import {encodeRewardPageClaim} from '../sdk/era-staking-rewards.mjs';
import {encodeAllocatedAssetCall} from '../sdk/era-v14-assets-client.mjs';
import {encodeEraV14Call} from '../sdk/era-v14-native.mjs';import {encodeAmmCall} from '../sdk/era-v14-amm.mjs';import {encodeNftMarketCall} from '../sdk/era-v14-nft-market.mjs';import {encodeModelRegistration,encodeModelApproval,encodeModelDelegation,encodeEvaluationSubmission,encodeEvaluationEvidence} from '../sdk/era-v14-evaluation.mjs';import {decodeEraAccount,parseEtkn} from '../sdk/era-account.mjs';
const field=(key,label,value='')=>({key,label,value});const asset=v=>{if(v==='ETKN')return 'NativeEtkn';if(!/^[1-9][0-9]*$/.test(v))throw new Error('Enter ETKN or a positive registered asset ID');return {Registered:Number(v)};};
const pair=[field('a','First asset: ETKN or registered asset ID','ETKN'),field('b','Second asset ID','7')];const end=field('deadline','Last valid block (maximum current block + 14,400)');
const recipient=field('recipient','Recipient ERA address');const native=(p,m,f,convert=x=>x)=>({label:p+' · '+m,fields:f,encode:v=>encodeEraV14Call(p,m,convert(v))});
export const workflows=[
 {label:'Send ETKN',fields:[recipient,field('amount','ETKN amount')],encode:v=>encodeEraV14Call('Balances','transferKeepAlive',{dest:decodeEraAccount(v.recipient),amount:parseEtkn(v.amount)})},
 ...[
 ['Create liquidity pool','createPool',[]],
 ['Provide liquidity','addLiquidity',[field('desiredA','First asset amount, base units'),field('desiredB','Second asset amount, base units'),field('minA','Minimum first asset accepted, base units'),field('minB','Minimum second asset accepted, base units')]],
 ['Remove liquidity','removeLiquidity',[field('lp','LP accounting units to remove'),field('minA','Minimum first asset received, base units'),field('minB','Minimum second asset received, base units'),recipient]],
 ['Swap exact input','swapExactInput',[field('amountIn','Input amount, base units'),field('minOut','Minimum output, base units'),recipient]],
 ['Swap exact output','swapExactOutput',[field('amountOut','Output amount, base units'),field('maxIn','Maximum input, base units'),recipient]],
 ].map(([label,method,fields])=>({label,fields:[...pair,...fields,end],encode:v=>encodeAmmCall(method,{...v,assetA:asset(v.a),assetB:asset(v.b),recipient:v.recipient?decodeEraAccount(v.recipient):undefined})})),
 ...[
 ['Set NFT price','setPrice',[field('collection','Collection ID'),field('item','Item ID'),field('price','Price in ETKN; blank removes listing'),field('buyer','Optional permitted buyer ERA address')]],
 ['Buy NFT','buyItem',[field('collection','Collection ID'),field('item','Item ID'),field('bidPrice','Maximum ETKN bid')]],
 ['Offer NFT swap','createSwap',[field('offeredCollection','Offered collection'),field('offeredItem','Offered item'),field('desiredCollection','Desired collection'),field('desiredItem','Desired item'),field('amount','ETKN accompanying swap; blank for no payment'),field('direction','Offer payment direction: Send or Receive','Receive'),field('duration','Offer duration in blocks')]],
 ['Cancel NFT swap','cancelSwap',[field('offeredCollection','Offered collection'),field('offeredItem','Offered item')]],
 ['Claim NFT swap','claimSwap',[field('sendCollection','Collection sent'),field('sendItem','Item sent'),field('receiveCollection','Collection received'),field('receiveItem','Item received'),field('amount','Exact ETKN price of offer; blank for no payment'),field('direction','Exact offer payment direction: Send or Receive','Receive')]],
 ].map(([label,method,fields])=>({label,fields,encode:v=>encodeNftMarketCall(method,{...v,buyer:v.buyer?decodeEraAccount(v.buyer):null,price:method==='setPrice'?(v.price?parseEtkn(v.price):null):v.amount?{amount:parseEtkn(v.amount),direction:v.direction}:null,bidPrice:v.bidPrice?parseEtkn(v.bidPrice):undefined,witnessPrice:v.amount?{amount:parseEtkn(v.amount),direction:v.direction}:null})})),
 {label:'Create admitted NFT collection',fields:['issuer','admin','freezer'].map(key=>field(key,key+' ERA address')),encode:v=>encodeAllocatedAssetCall('createCollection',v)},
 {label:'Mint next allocated NFT item',fields:[field('collection','Admitted collection ID'),recipient],encode:v=>encodeAllocatedAssetCall('mintItem',v)},
 native('Assets','transferKeepAlive',[field('id','Registered asset ID'),recipient,field('amount','Amount in asset base units')],v=>({...v,target:decodeEraAccount(v.recipient)})),
 native('EraWorlds','registerWorld',[field('worldId','World identifier'),field('commitment','SHA-256 commitment to content, 0x + 64 hex digits')]),
 native('EraWorlds','updateCommitment',[field('worldId','World identifier'),field('commitment','New content commitment')]),
 native('EraWorlds','deregisterWorld',[field('worldId','World identifier')]),
 {label:'Register off-chain model',fields:[field('artifactHash','Approved model-manifest SHA-256'),field('uri','Immutable model-manifest URI')],encode:v=>encodeModelRegistration(v.artifactHash,v.uri)},
 {label:'Approve model as independent reviewer',fields:[field('modelId','Model ID')],encode:v=>encodeModelApproval(v.modelId)},
 {label:'Set or revoke model submitter',fields:[field('modelId','Model ID'),field('delegate','Submitter ERA address; blank revokes')],encode:v=>encodeModelDelegation(v.modelId,v.delegate?decodeEraAccount(v.delegate):null)},
 {label:'Submit nonfinancial prediction',fields:['modelId','requestId','artifactHash','predictionHash','uri','expiresAt','cutoffUtc','evidenceAfterUtc'].map(key=>field(key,({modelId:'Model ID',requestId:'Scheduled request SHA-256',artifactHash:'Approved artifact SHA-256',predictionHash:'Exact prediction-envelope SHA-256',uri:'Immutable envelope URI',expiresAt:'Registry expiry block',cutoffUtc:'Submission cutoff, UTC epoch seconds',evidenceAfterUtc:'Earliest outcome evidence, UTC epoch seconds'})[key])),encode:v=>encodeEvaluationSubmission(v)},
 {label:'Record independent outcome or correction',fields:[field('predictionId','Prediction ID'),field('revision','Evidence revision: 0 original; 1–31 corrections'),field('evidenceHash','Exact evidence-envelope SHA-256'),field('outcome','Successful, Failed, or Inconclusive')],encode:v=>encodeEvaluationEvidence({...v,revision:Number(v.revision)})},
 {label:'Create admitted fungible asset',fields:['issuer','admin','freezer'].map(key=>field(key,key+' ERA address')),encode:v=>encodeAllocatedAssetCall('createAsset',v)},
 native('Assets','mint',[field('id','Registered asset ID'),recipient,field('amount','Amount in asset base units')],v=>({...v,beneficiary:decodeEraAccount(v.recipient)})),
 native('Assets','burn',[field('id','Registered asset ID'),field('who','Holder ERA address'),field('amount','Amount in asset base units')],v=>({...v,who:decodeEraAccount(v.who)})),
 native('Assets','setMetadata',[field('id','Registered asset ID'),field('name','Asset name'),field('symbol','Asset symbol'),field('decimals','Asset decimals')]),
 native('Assets','clearMetadata',[field('id','Registered asset ID')]),
 native('Assets','approveTransfer',[field('id','Registered asset ID'),field('delegate','Delegate ERA address'),field('amount','Allowance in asset base units')],v=>({...v,delegate:decodeEraAccount(v.delegate)})),
 native('Assets','cancelApproval',[field('id','Registered asset ID'),field('delegate','Delegate ERA address')],v=>({...v,delegate:decodeEraAccount(v.delegate)})),
 native('Assets','transferApproved',[field('id','Registered asset ID'),field('owner','Owner ERA address'),recipient,field('amount','Amount in asset base units')],v=>({...v,owner:decodeEraAccount(v.owner),destination:decodeEraAccount(v.recipient)})),
 native('Nfts','transfer',[field('collection','Collection ID'),field('item','Item ID'),recipient],v=>({...v,dest:decodeEraAccount(v.recipient)})),
 ...['burn','lockItemTransfer','unlockItemTransfer'].map(m=>native('Nfts',m,[field('collection','Collection ID'),field('item','Item ID')])),
 native('Nfts','setMetadata',[field('collection','Collection ID'),field('item','Item ID'),field('data','Item metadata text')]),
 native('Nfts','startCollectionRetirement',[field('collection','Empty collection ID to retire permanently')]),
 native('Nfts','continueCollectionRetirement',[field('collection','Retiring collection ID'),field('limit','Cleanup limit: 1–64')]),
 native('Nfts','continueDelegateCleanup',[field('collection','Collection ID'),field('item','Item ID'),field('delegate','Delegate ERA address'),field('limit','Cleanup limit: 1–64')],v=>({...v,delegate:decodeEraAccount(v.delegate)})),

 {label:'Claim consensus staking reward page',fields:[field('era','Completed, unexpired reward era'),field('validator','Historical validator ERA address'),field('page','Historical exposure page','0')],encode:encodeRewardPageClaim},
];

// Price tolerances are explicitly selected by the user, not an economic-policy default.
export async function quoteWorkflow(amm,workflow,values,basisPoints){
 const {slippageLimit}=await import('../sdk/era-v14-amm-client.mjs');
 if(![4,5].includes(workflow))throw new Error('Choose a swap workflow before quoting');
 if(!/^(0|[1-9][0-9]*)$/.test(String(basisPoints))||Number(basisPoints)>9999)throw new Error('Enter explicit slippage in basis points, 0–9999');
 const exact=workflow===4?'input':'output';
 const quote=await amm.quote(asset(values.a),asset(values.b),values[workflow===4?'amountIn':'amountOut'],exact);
 const bound=slippageLimit(quote.value[workflow===4?'amountOut':'amountIn'],Number(basisPoints),exact);
 if(bound===0n)throw new Error('Quote tolerance rounds to zero; use an executable positive bound');
 return {quote,field:workflow===4?'minOut':'maxIn',bound:bound.toString(),deadline:String(quote.height+64)};
}
