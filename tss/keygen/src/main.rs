#![allow(non_snake_case)]
use rand::prelude::*;
use curv::arithmetic::traits::Converter;
use curv::cryptographic_primitives::proofs::sigma_dlog::DLogProof;
use curv::cryptographic_primitives::secret_sharing::feldman_vss::*;
use curv::elliptic::curves::traits::*;
use curv::{FE, GE};
use keygens::protocols::multi_party_ecdsa::gg_2020::orchestrate::*;
use keygens::protocols::multi_party_ecdsa::gg_2020::party_i::{
    KeyGenBroadcastMessage1, KeyGenDecommitMessage1, Parameters, SharedKeys,
};
use paillier::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{env, fs, time};
use zk_paillier::zkproofs::DLogStatement;
use paillier::{EncryptionKey, DecryptionKey};
mod common;
use common::{
    aes_decrypt, aes_encrypt, broadcast, poll_for_broadcasts, poll_for_p2p, postb, sendp2p, Params,
    PartySignup, AEAD,
};
use curv::cryptographic_primitives::hashing::hash_sha256::HSha256;
use curv::cryptographic_primitives::hashing::traits::Hash;
impl From<Params> for Parameters {
    fn from(item: Params) -> Self {
        Parameters {
            share_count: item.parties.parse::<u16>().unwrap(),
            threshold: item.threshold.parse::<u16>().unwrap(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivateKey{
    pub ek: EncryptionKey,
    pub dk: DecryptionKey
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fileinfo{
    pub originalName: String,
    pub reshareTimes: u16,
    pub currentName: BigInt
}

fn main() {
    if env::args().nth(3).is_some() {
        panic!("too many arguments")
    }
    if env::args().nth(2).is_none() {
        panic!("too few arguments")
    }

    let params: Parameters = serde_json::from_str::<Params>(
        &std::fs::read_to_string("params.json").expect("Could not read input params file"),
    )
    .unwrap()
    .into();
    
    let client = Client::new();
    let (party_num_int, uuid) = match signup(&client).unwrap() {
        PartySignup { number, uuid } => (number, uuid),
    };
    let delay = time::Duration::from_millis(25);
    let input_stage1 = KeyGenStage1Input {
        index: (party_num_int - 1) as usize,
    };
    let res_stage1: KeyGenStage1Result = keygen_stage1(&input_stage1);
    let mut rng = rand::thread_rng();
    let bkx = rng.gen::<u32>();
    let mut other_bks:Vec<u32> = Vec::new();
    let mut bks:Vec<u32> = Vec::new();
    assert!(broadcast(
            &client,
            party_num_int,
            "round0",
            bkx.clone().to_string(),
            uuid.clone(),
            ).is_ok());
    let round0_ans_vec = poll_for_broadcasts(&client,party_num_int,params.share_count,delay,"round0",uuid.clone());
    for i in 0..round0_ans_vec.len() {
        let tempResult = round0_ans_vec[i].clone().parse::<u32>();
        let temp = match tempResult{
            Ok(s) =>s,
            Err(error) => {
                panic!("{:?}",error)
            }
        };
        other_bks.push(temp);
    }
    let mut ix = 0;
    for i in 0..params.share_count {
        if i == party_num_int-1 {
            bks.push(bkx.clone());
        } else {
            bks.push(other_bks[ix].clone());
            ix = ix + 1;
        }
    }

    assert!(broadcast(
        &client,
        party_num_int,
        "round1",
        serde_json::to_string(&res_stage1.bc_com1_l).unwrap(),
        uuid.clone()
    )
    .is_ok());
    let round1_ans_vec = poll_for_broadcasts(
        &client,
        party_num_int,
        params.share_count,
        delay,
        "round1",
        uuid.clone(),
    );

    let mut bc1_vec = round1_ans_vec
        .iter()
        .map(|m| serde_json::from_str::<KeyGenBroadcastMessage1>(m).unwrap())
        .collect::<Vec<_>>();

    bc1_vec.insert(party_num_int as usize - 1, res_stage1.bc_com1_l);
    assert!(broadcast(
        &client,
        party_num_int,
        "round2",
        serde_json::to_string(&res_stage1.decom1_l).unwrap(),
        uuid.clone()
    )
    .is_ok());
    let round2_ans_vec = poll_for_broadcasts(
        &client,
        party_num_int,
        params.share_count,
        delay,
        "round2",
        uuid.clone(),
    );
    let mut decom1_vec = round2_ans_vec
        .iter()
        .map(|m| serde_json::from_str::<KeyGenDecommitMessage1>(m).unwrap())
        .collect::<Vec<_>>();
    decom1_vec.insert(party_num_int as usize - 1, res_stage1.decom1_l);
    let input_stage2 = KeyGenStage2Input {
        index: (party_num_int - 1) as usize,
        params_s: params.clone(),
        party_keys_s: res_stage1.party_keys_l.clone(),
        decom1_vec_s: decom1_vec.clone(),
        bc1_vec_s: bc1_vec.clone(),
    };
    let res_stage2 = keygen_stage2(&input_stage2,&bks).expect("keygen stage 2 failed.");
    let mut point_vec: Vec<GE> = Vec::new();
    let mut enc_keys: Vec<BigInt> = Vec::new();
    for i in 1..=params.share_count {
        point_vec.push(decom1_vec[(i - 1) as usize].y_i);
        if i != party_num_int {
            enc_keys.push(
                (decom1_vec[(i - 1) as usize].y_i.clone() * res_stage1.party_keys_l.u_i)
                    .x_coor()
                    .unwrap(),
            );
        }
    }

    let (head, tail) = point_vec.split_at(1);
    let y_sum = tail.iter().fold(head[0], |acc, x| acc + x);
    let mut j = 0;
    for (k, i) in (1..=params.share_count).enumerate() {
        if i != party_num_int {
            // prepare encrypted ss for party i:
            let key_i = BigInt::to_vec(&enc_keys[j]);
            let plaintext = BigInt::to_vec(&res_stage2.secret_shares_s[k].to_big_int());
            let aead_pack_i = aes_encrypt(&key_i, &plaintext);
            // This client does not implement the identifiable abort protocol.
            // If it were these secret shares would need to be broadcasted to indetify the
            // malicious party.
            assert!(sendp2p(
                &client,
                party_num_int,
                i,
                "round3",
                serde_json::to_string(&aead_pack_i).unwrap(),
                uuid.clone()
            )
            .is_ok());
            j += 1;
        }
    }
    // get shares from other parties.
    let round3_ans_vec = poll_for_p2p(
        &client,
        party_num_int,
        params.share_count,
        delay,
        "round3",
        uuid.clone(),
    );
    // decrypt shares from other parties.
    let mut j = 0;
    let mut party_shares: Vec<FE> = Vec::new();
    for i in 1..=params.share_count {
        if i == party_num_int {
            party_shares.push(res_stage2.secret_shares_s[(i - 1) as usize]);
        } else {
            let aead_pack: AEAD = serde_json::from_str(&round3_ans_vec[j]).unwrap();
            let key_i = BigInt::to_vec(&enc_keys[j]);
            let out = aes_decrypt(&key_i, aead_pack);
            let out_bn = BigInt::from(&out[..]);
            let out_fe = ECScalar::from(&out_bn);
            party_shares.push(out_fe);

            j += 1;
        }
    }
    assert!(broadcast(
        &client,
        party_num_int,
        "round4",
        serde_json::to_string(&res_stage2.vss_scheme_s).unwrap(),
        uuid.clone()
    )
    .is_ok());
    //get vss_scheme for others.
    let round4_ans_vec = poll_for_broadcasts(
        &client,
        party_num_int,
        params.share_count,
        delay,
        "round4",
        uuid.clone(),
    );

    let mut j = 0;
    let mut vss_scheme_vec: Vec<VerifiableSS> = Vec::new();
    for i in 1..=params.share_count {
        if i == party_num_int {
            vss_scheme_vec.push(res_stage2.vss_scheme_s.clone());
        } else {
            let vss_scheme_j: VerifiableSS = serde_json::from_str(&round4_ans_vec[j]).unwrap();
            vss_scheme_vec.push(vss_scheme_j);
            j += 1;
        }
    }
    let input_stage3 = KeyGenStage3Input {
        party_keys_s: res_stage1.party_keys_l.clone(),
        vss_scheme_vec_s: vss_scheme_vec.clone(),
        secret_shares_vec_s: party_shares,
        y_vec_s: point_vec.clone(),
        index_s: (bkx.clone() - 1) as usize,
        params_s: params.clone(),
    };
    let res_stage3 = keygen_stage3(&input_stage3).expect("stage 3 keygen failed.");
    // round 5: send dlog proof
    assert!(broadcast(
        &client,
        party_num_int,
        "round5",
        serde_json::to_string(&res_stage3.dlog_proof_s).unwrap(),
        uuid.clone()
    )
    .is_ok());
    let round5_ans_vec = poll_for_broadcasts(
        &client,
        party_num_int,
        params.share_count,
        delay,
        "round5",
        uuid.clone(),
    );

    let mut j = 0;
    let mut dlog_proof_vec: Vec<DLogProof> = Vec::new();
    for i in 1..=params.share_count {
        if i == party_num_int {
            dlog_proof_vec.push(res_stage3.dlog_proof_s.clone());
        } else {
            let dlog_proof_j: DLogProof = serde_json::from_str(&round5_ans_vec[j]).unwrap();
            dlog_proof_vec.push(dlog_proof_j);
            j += 1;
        }
    }

    let input_stage4 = KeyGenStage4Input {
        params_s: params.clone(),
        dlog_proof_vec_s: dlog_proof_vec.clone(),
        y_vec_s: point_vec.clone(),
    };
    let _ = keygen_stage4(&input_stage4).expect("keygen stage4 failed.");
    //save key to file:
    let paillier_key_vec = (0..params.share_count)
        .map(|i| bc1_vec[i as usize].e.clone())
        .collect::<Vec<EncryptionKey>>();
    let h1_h2_N_tilde_vec = bc1_vec
        .iter()
        .map(|bc1| bc1.dlog_statement.clone())
        .collect::<Vec<DLogStatement>>();
    let temp_private_key = PrivateKey{
        ek: res_stage1.party_keys_l.ek.clone(),
        dk: res_stage1.party_keys_l.dk.clone()
    };
    let currentname_str = env::args().nth(2).unwrap();
    let currentname = match hex::decode(currentname_str.clone()) {
        Ok(x) => x,
        Err(_e) => currentname_str.as_bytes().to_vec(),
    };
    let currentname = &currentname[..];
    let fileinfos = Fileinfo {
        originalName: env::args().nth(2).unwrap(),
        reshareTimes: 0 as u16,
        currentName: HSha256::create_hash(&[&BigInt::from(currentname)])
    };
    let party_key_pair = FileKeyPair {
        party_keys_s: temp_private_key,
        shared_keys: res_stage3.shared_keys_s.clone(),
        paillier_key: paillier_key_vec[(party_num_int.clone() - 1) as usize].clone(),
        y_sum_s: y_sum,
        h1_h2_N_tilde: h1_h2_N_tilde_vec[(party_num_int.clone() - 1) as usize].clone(),
        bks: bkx.clone(),
        fileinfo: fileinfos
    };
    let mut fileName = env::args().nth(2).unwrap();
    fileName.push_str("-0");
    fs::write(
        fileName,
        serde_json::to_string(&party_key_pair).unwrap(),
    )
    .expect("Unable to save !");
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileKeyPair {
    pub party_keys_s: PrivateKey,
    pub shared_keys: SharedKeys,
    pub y_sum_s: GE,
    pub paillier_key: EncryptionKey,
    pub h1_h2_N_tilde: DLogStatement,
    pub bks: u32,
    pub fileinfo: Fileinfo
}
pub fn signup(client: &Client) -> Result<PartySignup, ()> {
    let key = "signup-keygen".to_string();
    
    let res_body = postb(&client, "signupkeygen", key).unwrap();
    serde_json::from_str(&res_body).unwrap()
}
