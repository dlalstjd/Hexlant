import Web3 from 'web3'
import _Tx from 'ethereumjs-tx'
import keccak from 'js-sha3'
//import secp256k1 from 'secp256k1'
import pkg from 'ethereumjs-wallet'
import txDecoder from 'ethereum-tx-decoder'
//import util from 'ethereumjs-util'
import rlp from 'rlp'

const url = 'https://ropsten.infura.io/v3/375a84d45ba0456a8d39a32cce31471c'
const web3 = new Web3(new Web3.providers.HttpProvider(url));
const { Transaction : Tx } = _Tx
const{ keccak256 } = keccak
const { default : Wallet } = pkg

const from_address = "0x8524e72ff6a0a24b8c1c67ca74577ba5046b0183"
const to_address = "0x743376fd2a693723A60942D0b4B2F1765ea1Dbb0"

const rawTransaction = {
    from: from_address,
    to: to_address,
    nonce: web3.utils.toHex(1),
    value: web3.utils.toHex(web3.utils.toWei('0.001', 'ether')),
    gasLimit: web3.utils.toHex(21000),
    gasPrice: web3.utils.toHex(1000000000),//50e9
};

var tx = new Tx(rawTransaction, { chain: 'ropsten', hardfork: 'petersburg' }, )
//console.log(keccak(rlp.encode(tx.raw)))
/*
const r = "50595355918819243410031121052044548209002512062447742184422428765482868356793"
const s = "55171353374886157105762314656588687649239061634763395144694786161377614195084"
const v = 41

tx.r = web3.utils.toHex(r)
tx.s = web3.utils.toHex(s)
tx.v = web3.utils.toHex(v)

//console.log(tx.raw)
//console.log(tx.getSenderAddress())
const serializedTx = tx.serialize()
// console.log(serializedTx)

const raw = '0x' + serializedTx.toString('hex')
//console.log(raw);

//console.log(txDecoder.decodeTx(tx))
web3.eth.sendSignedTransaction( raw, (err, txHash) => {
    console.log('err:', err, 'txHash: ', txHash)
})
*/

//-----------------------------------------------------------------------------------------------//
// private key reconstruction from wi

const x = "9B49E0BD55968FABBEBCC10EF9E1A60D3A3547C6A656912B4D3DDF9C6386D299"

tx.sign(Uint8Array.from(Buffer.from(x, 'hex')))
//console.log(tx.r)
//console.log(tx.s)

console.log(tx.getSenderAddress())
//console.log(tx.raw)

const serializedTx = tx.serialize()
const raw = '0x' + serializedTx.toString('hex')

//console.log(txDecoder.decodeTx(tx))
web3.eth.sendSignedTransaction( raw, (err, txHash) => {
    console.log('err:', err, 'txHash: ', txHash)
})
