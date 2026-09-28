import {encodeAmmCall} from '../sdk/era-v14-amm.mjs';import {encodeNftMarketCall} from '../sdk/era-v14-nft-market.mjs';
const common={assetA:'NativeEtkn',assetB:{Registered:7},assetIn:'NativeEtkn',assetOut:{Registered:7},deadline:110,recipient:'0x'+'02'.repeat(32)};
const calls={};
for(const [method,args] of Object.entries({createPool:{},addLiquidity:{desiredA:100,desiredB:200,minA:1,minB:2},removeLiquidity:{lp:50,minA:1,minB:2},swapExactInput:{amountIn:10,minOut:9},swapExactOutput:{amountOut:9,maxIn:10}}))calls[method]=encodeAmmCall(method,{...common,...args});
for(const [method,args] of Object.entries({setPrice:{collection:1,item:2,price:5,buyer:'0x'+'02'.repeat(32)},buyItem:{collection:1,item:2,bidPrice:6},createSwap:{offeredCollection:1,offeredItem:2,desiredCollection:3,desiredItem:4,price:{amount:5,direction:'Receive'},duration:100},cancelSwap:{offeredCollection:1,offeredItem:2},claimSwap:{sendCollection:3,sendItem:4,receiveCollection:1,receiveItem:2,witnessPrice:{amount:5,direction:'Receive'}}}))calls[method]=encodeNftMarketCall(method,args);
console.log(JSON.stringify(calls,null,2));
