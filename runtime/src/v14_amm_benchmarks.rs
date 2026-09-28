//! Runtime AMM workflow benchmarks. Test asset7 is never a commissioning proposal.
use crate::{Runtime,AccountId,RuntimeOrigin,System,Balances,Assets,Amm,DECIMALS};
use core::marker::PhantomData;
use alloc::{vec,vec::Vec};
use frame_benchmarking::v1::{account,benchmarks};
use frame_support::{assert_ok,traits::Currency};
use era_v14_amm::v1::Asset::{Native,Registered};
use sp_runtime::MultiAddress;
pub use frame_system::Config;
pub struct Pallet<T:Config>(PhantomData<T>);
fn who()->AccountId {account("amm-benchmark",0,0)}
fn setup(pool:bool,liquidity:bool) {
 System::set_block_number(100);let _=Balances::make_free_balance_be(&who(),1_000_000*DECIMALS);
 assert_ok!(Assets::create(RuntimeOrigin::signed(who()),7,MultiAddress::Id(who()),1));
 assert_ok!(Assets::mint(RuntimeOrigin::signed(who()),7,MultiAddress::Id(who()),1_000_000*DECIMALS));
 assert_ok!(Amm::approve_pair(RuntimeOrigin::root(),Native,Registered(7)));assert_ok!(Amm::activate(RuntimeOrigin::root()));
 if pool {assert_ok!(Amm::create_pool(RuntimeOrigin::signed(who()),Native,Registered(7),110));}
 if liquidity {assert_ok!(Amm::add_liquidity(RuntimeOrigin::signed(who()),Native,Registered(7),10_000*DECIMALS,10_000*DECIMALS,1,1,110));}
}

// Both admitted pair classes, with preserved native providers for non-sufficient assets.
fn registered_setup(pool: bool, liquidity: bool) {
 setup(false,false);
 assert_ok!(Assets::create(RuntimeOrigin::signed(who()),8,MultiAddress::Id(who()),1));
 assert_ok!(Assets::mint(RuntimeOrigin::signed(who()),8,MultiAddress::Id(who()),1_000_000*DECIMALS));
 assert_ok!(Amm::approve_pair(RuntimeOrigin::root(),Registered(7),Registered(8)));
 if pool {assert_ok!(Amm::create_pool(RuntimeOrigin::signed(who()),Registered(7),Registered(8),110));}
 if liquidity {assert_ok!(Amm::add_liquidity(RuntimeOrigin::signed(who()),Registered(7),Registered(8),10_000*DECIMALS,10_000*DECIMALS,1,1,110));}
}
fn recipient()->AccountId {let r=account("amm-new-recipient",0,0);let _=Balances::make_free_balance_be(&r,10*DECIMALS);r}
// Seed a reconciled maximum-sized provider index by dividing the existing user LP.
// Reserves, total/user/locked LP and issuance are unchanged. No production identities.
fn full_providers(pair:(era_v14_amm::v1::Asset,era_v14_amm::v1::Asset)) {
 use era_v14_amm::{Positions,Providers,AccountPools};
 let old=Positions::<Runtime>::get(pair,who()).unwrap();
 let mut providers=Vec::new();
 for n in 1..1024 {let a:AccountId=account("amm-provider",n,0);Positions::<Runtime>::insert(pair,&a,DECIMALS);AccountPools::<Runtime>::insert(&a,frame_support::BoundedVec::try_from(vec![pair]).unwrap());providers.push(a);}
 // Put the measured account last to exercise the full lookup.
 providers.push(who());Providers::<Runtime>::insert(pair,frame_support::BoundedVec::try_from(providers).unwrap());
 Positions::<Runtime>::insert(pair,who(),old-1023*DECIMALS);
 assert_ok!(era_v14_amm::Pallet::<Runtime>::reconcile(pair));
}
fn full_positions(pair:(era_v14_amm::v1::Asset,era_v14_amm::v1::Asset)) {
 // 64 positions are reachable using pair combinations of 13 assets. Respect the
 // runtime's consumer limit; force-created sufficient assets are intentionally forbidden.
 let mut assets=vec![Native];
 for id in 20..32 {
  assert_ok!(Assets::create(RuntimeOrigin::signed(who()),id,MultiAddress::Id(who()),1));
  assert_ok!(Assets::mint(RuntimeOrigin::signed(who()),id,MultiAddress::Id(who()),10_000*DECIMALS));
  assets.push(Registered(id));
 }
 let mut count=0;
 'pairs: for i in 0..assets.len() {for j in i+1..assets.len() {
  let a=assets[i];let b=assets[j];
  assert_ok!(Amm::approve_pair(RuntimeOrigin::root(),a,b));
  assert_ok!(Amm::create_pool(RuntimeOrigin::signed(who()),a,b,110));
  assert_ok!(Amm::add_liquidity(RuntimeOrigin::signed(who()),a,b,10*DECIMALS,10*DECIMALS,1,1,110));
  count+=1;if count==63 {break 'pairs;}
 }}
 let mut ps=era_v14_amm::AccountPools::<Runtime>::get(who()).into_inner();
 ps.rotate_left(1); // Target is last; no duplicate or orphan index entries.
 era_v14_amm::AccountPools::<Runtime>::insert(who(),frame_support::BoundedVec::try_from(ps).unwrap());
 full_providers(pair);
}
fn new_provider(pair:(era_v14_amm::v1::Asset,era_v14_amm::v1::Asset)) {
 full_positions(pair);
 let lp=era_v14_amm::Positions::<Runtime>::take(pair,who()).unwrap();
 let mut ps=era_v14_amm::Providers::<Runtime>::get(pair).into_inner();assert_eq!(ps.pop(),Some(who()));
 era_v14_amm::Positions::<Runtime>::mutate(pair,&ps[0],|v| *v=Some(v.unwrap()+lp));
 era_v14_amm::Providers::<Runtime>::insert(pair,frame_support::BoundedVec::try_from(ps).unwrap());
 let mut ap=era_v14_amm::AccountPools::<Runtime>::get(who()).into_inner();assert_eq!(ap.pop(),Some(pair));
 era_v14_amm::AccountPools::<Runtime>::insert(who(),frame_support::BoundedVec::try_from(ap).unwrap());
 assert_ok!(era_v14_amm::Pallet::<Runtime>::reconcile(pair));
}

benchmarks! {
 create_pool {setup(false,false);}: {Amm::create_pool(RuntimeOrigin::signed(who()),Native,Registered(7),110)?;}
 add_initial {setup(true,false);}: {Amm::add_liquidity(RuntimeOrigin::signed(who()),Native,Registered(7),10_000*DECIMALS,10_000*DECIMALS,1,1,110)?;}
 add_subsequent {setup(true,true);}: {Amm::add_liquidity(RuntimeOrigin::signed(who()),Native,Registered(7),100*DECIMALS,100*DECIMALS,1,1,110)?;}
 remove_liquidity {setup(true,true);}: {Amm::remove_liquidity(RuntimeOrigin::signed(who()),Native,Registered(7),100*DECIMALS,1,1,who(),110)?;}
 swap_exact_input {setup(true,true);}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Native,Registered(7),10*DECIMALS,1,who(),110)?;}
 swap_exact_output {setup(true,true);}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Native,Registered(7),10*DECIMALS,100*DECIMALS,who(),110)?;}

approve_registered {registered_setup(false,false); crate::v14_amm::ApprovedPairs::<Runtime>::remove((Registered(7),Registered(8)));}: {Amm::approve_pair(RuntimeOrigin::root(),Registered(7),Registered(8))?;}
activate {setup(false,false);crate::v14_amm::Enabled::<Runtime>::put(false);}: {Amm::activate(RuntimeOrigin::root())?;}
create_registered {registered_setup(false,false);}: {Amm::create_pool(RuntimeOrigin::signed(who()),Registered(7),Registered(8),110)?;}
add_registered_initial {registered_setup(true,false);}: {Amm::add_liquidity(RuntimeOrigin::signed(who()),Registered(7),Registered(8),10_000*DECIMALS,10_000*DECIMALS,1,1,110)?;}
add_native_bounded {setup(true,true);full_positions((Native,Registered(7)));}: {Amm::add_liquidity(RuntimeOrigin::signed(who()),Native,Registered(7),100*DECIMALS,200*DECIMALS,1,1,110)?;}
remove_native_bounded {setup(true,true);full_positions((Native,Registered(7)));let lp=era_v14_amm::Positions::<Runtime>::get((Native,Registered(7)),who()).unwrap();let r=recipient();}: {Amm::remove_liquidity(RuntimeOrigin::signed(who()),Native,Registered(7),lp,1,1,r,110)?;}
swap_native_input_0_0 {setup(true,true);let r=who();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Native,Registered(7),10*DECIMALS,1,r,110)?;}
swap_native_input_0_1 {setup(true,true);let r=recipient();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Native,Registered(7),10*DECIMALS,1,r,110)?;}
swap_native_output_0_0 {setup(true,true);let r=who();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Native,Registered(7),10*DECIMALS,100*DECIMALS,r,110)?;}
swap_native_output_0_1 {setup(true,true);let r=recipient();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Native,Registered(7),10*DECIMALS,100*DECIMALS,r,110)?;}
swap_native_input_1_0 {setup(true,true);let r=who();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Registered(7),Native,10*DECIMALS,1,r,110)?;}
swap_native_input_1_1 {setup(true,true);let r=recipient();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Registered(7),Native,10*DECIMALS,1,r,110)?;}
swap_native_output_1_0 {setup(true,true);let r=who();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Registered(7),Native,10*DECIMALS,100*DECIMALS,r,110)?;}
swap_native_output_1_1 {setup(true,true);let r=recipient();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Registered(7),Native,10*DECIMALS,100*DECIMALS,r,110)?;}
add_registered_bounded {registered_setup(true,true);full_positions((Registered(7),Registered(8)));}: {Amm::add_liquidity(RuntimeOrigin::signed(who()),Registered(7),Registered(8),100*DECIMALS,200*DECIMALS,1,1,110)?;}
remove_registered_bounded {registered_setup(true,true);full_positions((Registered(7),Registered(8)));let lp=era_v14_amm::Positions::<Runtime>::get((Registered(7),Registered(8)),who()).unwrap();let r=recipient();}: {Amm::remove_liquidity(RuntimeOrigin::signed(who()),Registered(7),Registered(8),lp,1,1,r,110)?;}
swap_registered_input_0_0 {registered_setup(true,true);let r=who();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Registered(7),Registered(8),10*DECIMALS,1,r,110)?;}
swap_registered_input_0_1 {registered_setup(true,true);let r=recipient();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Registered(7),Registered(8),10*DECIMALS,1,r,110)?;}
swap_registered_output_0_0 {registered_setup(true,true);let r=who();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Registered(7),Registered(8),10*DECIMALS,100*DECIMALS,r,110)?;}
swap_registered_output_0_1 {registered_setup(true,true);let r=recipient();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Registered(7),Registered(8),10*DECIMALS,100*DECIMALS,r,110)?;}
swap_registered_input_1_0 {registered_setup(true,true);let r=who();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Registered(8),Registered(7),10*DECIMALS,1,r,110)?;}
swap_registered_input_1_1 {registered_setup(true,true);let r=recipient();}: {Amm::swap_exact_input(RuntimeOrigin::signed(who()),Registered(8),Registered(7),10*DECIMALS,1,r,110)?;}
swap_registered_output_1_0 {registered_setup(true,true);let r=who();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Registered(8),Registered(7),10*DECIMALS,100*DECIMALS,r,110)?;}
swap_registered_output_1_1 {registered_setup(true,true);let r=recipient();}: {Amm::swap_exact_output(RuntimeOrigin::signed(who()),Registered(8),Registered(7),10*DECIMALS,100*DECIMALS,r,110)?;}

 add_native_new_bounded {setup(true,true);new_provider((Native,Registered(7)));}: {Amm::add_liquidity(RuntimeOrigin::signed(who()),Native,Registered(7),200*DECIMALS,100*DECIMALS,1,1,110)?;}
 add_registered_new_bounded {registered_setup(true,true);new_provider((Registered(7),Registered(8)));}: {Amm::add_liquidity(RuntimeOrigin::signed(who()),Registered(7),Registered(8),200*DECIMALS,100*DECIMALS,1,1,110)?;}
}
