//! Incremental Apply measurements; existing proof/observer worst-case weights are retained.
//! Fixtures are synthetic and do not select a production penalty policy.
use crate::{AccountId,Runtime,RuntimeOrigin,System,Session,Staking,Balances,DECIMALS};
use alloc::{vec,vec::Vec};
use core::marker::PhantomData;
use frame_benchmarking::v1::{account,benchmarks};
use frame_support::{assert_ok,traits::Currency,storage::with_storage_layer};
use sp_runtime::{Perbill,DispatchError};
pub use frame_system::Config;
pub struct Pallet<T:Config>(PhantomData<T>);
fn who(n:u32)->AccountId {account("penalty-apply",n,0)}
fn setup(queue:u32,prior:bool) {
 System::set_block_number(20000);
 let treasury=crate::AiPenaltyDestination::get();let _=Balances::make_free_balance_be(&treasury,100*DECIMALS);
 let validators:Vec<_>=(0..16).map(who).collect();pallet_session::Validators::<Runtime>::put(validators.clone());
 for v in validators {let _=Balances::make_free_balance_be(&v,50_000*DECIMALS);assert_ok!(Staking::bond(RuntimeOrigin::signed(v),10_000*DECIMALS,pallet_staking::RewardDestination::Stash));}
 let mut others=Vec::new();
 for n in 100..164 {let v=who(n);let _=Balances::make_free_balance_be(&v,50_000*DECIMALS);assert_ok!(Staking::bond(RuntimeOrigin::signed(v.clone()),30_000*DECIMALS,pallet_staking::RewardDestination::Stash));others.push(pallet_staking::IndividualExposure{who:v,value:30_000*DECIMALS/16});}
 pallet_session::CurrentIndex::<Runtime>::put(143);
 pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo{index:23,start:Some(0)});
 pallet_staking::CurrentEra::<Runtime>::put(23);
 pallet_staking::BondedEras::<Runtime>::put((0..24).map(|e|(e,e*6)).collect::<Vec<_>>());
 for e in 0..24 {pallet_staking::ErasStartSessionIndex::<Runtime>::insert(e,e*6);}
 pallet_staking::Invulnerables::<Runtime>::kill();pallet_staking::SlashRewardFraction::<Runtime>::put(Perbill::zero());
 pallet_staking::ErasStakersOverview::<Runtime>::insert(0,who(0),sp_staking::PagedExposureMetadata{total:10_000*DECIMALS+64*(30_000*DECIMALS/16),own:10_000*DECIMALS,nominator_count:64,page_count:1});
 pallet_staking::ErasStakersPaged::<Runtime>::insert((0,who(0),0),sp_staking::ExposurePage{page_total:64*(30_000*DECIMALS/16),others:others.clone()});
 crate::v14_penalties::Policy::<Runtime>::put(crate::v14_penalties::PolicyV1{maximum_fraction_ppb:10_000_000,activation_session:0});
 if prior {pallet_staking::ValidatorSlashInEra::<Runtime>::insert(0,who(0),(Perbill::from_percent(1),100*DECIMALS));}
 let pending:Vec<_>=(0..queue).map(|i|{let n=if prior {i} else {i+1};pallet_staking::UnappliedSlash{validator:who(n),own:100*DECIMALS,others:others.iter().map(|x|(x.who.clone(),300*DECIMALS/16)).collect(),reporters:vec![],payout:0}}).collect();
 pallet_staking::UnappliedSlashes::<Runtime>::insert(24,pending);
}
fn history(prune:bool) {
 let spans=pallet_staking::slashing::SlashingSpans {span_index:25,last_start:25,last_nonzero_slash:24,prior:vec![1;25]};
 for n in core::iter::once(0).chain(100..164) {pallet_staking::SlashingSpans::<Runtime>::insert(who(n),spans.clone());}
 let (active,era)=if prune {(49,26)} else {(24,1)};
 pallet_session::CurrentIndex::<Runtime>::put(active*6+5);
 pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo{index:active,start:Some(0)});
 pallet_staking::CurrentEra::<Runtime>::put(active);
 pallet_staking::BondedEras::<Runtime>::put(((active-24)..=active).map(|e|(e,e*6)).collect::<Vec<_>>());
 pallet_staking::ErasStartSessionIndex::<Runtime>::insert(active,active*6);
 let overview=pallet_staking::ErasStakersOverview::<Runtime>::get(0,who(0)).unwrap();
 let page=pallet_staking::ErasStakersPaged::<Runtime>::get((0,who(0),0)).unwrap();
 pallet_staking::ErasStakersOverview::<Runtime>::insert(era,who(0),overview);
 pallet_staking::ErasStakersPaged::<Runtime>::insert((era,who(0),0),page);
 pallet_staking::UnappliedSlashes::<Runtime>::insert(era+24,pallet_staking::UnappliedSlashes::<Runtime>::get(24));
}
fn apply_at(session:u32)->Result<(),DispatchError> {
 with_storage_layer(||crate::v14_penalties::after_verified_report(who(0),session,0,16,true).map_err(|_|DispatchError::Other("penalty apply rejected")))
}
fn apply()->Result<(),DispatchError> {apply_at(0)}
fn consensus_fixture()->(crate::BabeObservationEvidence,crate::GrandpaObservationEvidence,AccountId,AccountId,u128,u128) {
 use era_validator_security::observation::BenchmarkHelper;
 type H=crate::RuntimeObserverBenchmarkHelper;
 let babe=H::setup_babe(4,0,0).expect("verified BABE fixture");let grandpa=H::setup_grandpa(4,0,0).expect("verified GRANDPA fixture");
 let offender=Session::validators()[0].clone();let nominator=who(200);let treasury=crate::AiPenaltyDestination::get();let _=Balances::make_free_balance_be(&treasury,100*DECIMALS);
 for (id,stake) in [(offender.clone(),10_000u128),(nominator.clone(),30_000u128)] {let _=Balances::make_free_balance_be(&id,50_000*DECIMALS);assert_ok!(Staking::bond(RuntimeOrigin::signed(id),stake*DECIMALS,pallet_staking::RewardDestination::Stash));}
 pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo{index:0,start:Some(0)});pallet_staking::CurrentEra::<Runtime>::put(0);pallet_staking::ErasStartSessionIndex::<Runtime>::insert(0,0);pallet_staking::BondedEras::<Runtime>::put(vec![(0,0)]);pallet_staking::Invulnerables::<Runtime>::kill();pallet_staking::SlashRewardFraction::<Runtime>::put(Perbill::zero());
 pallet_staking::ErasStakersOverview::<Runtime>::insert(0,&offender,sp_staking::PagedExposureMetadata{total:40_000*DECIMALS,own:10_000*DECIMALS,nominator_count:1,page_count:1});pallet_staking::ErasStakersPaged::<Runtime>::insert((0,&offender,0),sp_staking::ExposurePage{page_total:30_000*DECIMALS,others:vec![pallet_staking::IndividualExposure{who:nominator.clone(),value:30_000*DECIMALS}]});
 crate::v14_penalties::Policy::<Runtime>::put(crate::v14_penalties::PolicyV1{maximum_fraction_ppb:10_000_000,activation_session:0});let issuance=Balances::total_issuance();let balance=Balances::free_balance(&treasury);(babe,grandpa,offender,nominator,issuance,balance)
}
fn verified_reports(babe:crate::BabeObservationEvidence,grandpa:crate::GrandpaObservationEvidence,cancel:bool) {
 use frame_support::unsigned::ValidateUnsigned;
 use sp_runtime::transaction_validity::TransactionSource;
 let b=pallet_babe::Call::<Runtime>::report_equivocation_unsigned{equivocation_proof:alloc::boxed::Box::new(babe.0.clone()),key_owner_proof:babe.1.clone()};
 assert!(<crate::Babe as ValidateUnsigned>::validate_unsigned(TransactionSource::InBlock,&b).is_ok());assert_ok!(crate::Babe::report_equivocation_unsigned(RuntimeOrigin::none(),alloc::boxed::Box::new(babe.0),babe.1));assert!(<crate::Babe as ValidateUnsigned>::validate_unsigned(TransactionSource::InBlock,&b).is_err());assert_eq!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).len(),1);
 if cancel {assert_ok!(Staking::cancel_deferred_slash(RuntimeOrigin::root(),24,vec![0]));}
 let g=pallet_grandpa::Call::<Runtime>::report_equivocation_unsigned{equivocation_proof:alloc::boxed::Box::new(grandpa.0.clone()),key_owner_proof:grandpa.1.clone()};assert!(<crate::Grandpa as ValidateUnsigned>::validate_unsigned(TransactionSource::InBlock,&g).is_ok());assert_ok!(crate::Grandpa::report_equivocation_unsigned(RuntimeOrigin::none(),alloc::boxed::Box::new(grandpa.0),grandpa.1));assert!(<crate::Grandpa as ValidateUnsigned>::validate_unsigned(TransactionSource::InBlock,&g).is_err());assert_eq!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).len(),if cancel {0}else{1});
}
fn deferred_boundary(){use pallet_session::SessionManager;pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo{index:23,start:Some(0)});pallet_staking::ErasStartSessionIndex::<Runtime>::insert(24,144);<Staking as SessionManager<AccountId>>::start_session(144);}
benchmarks! {
 pipeline_apply {let (b,g,o,n,issuance,balance)=consensus_fixture();let validators=Session::validators();}: {verified_reports(b,g,false);deferred_boundary();}
 verify {assert!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).is_empty());assert_eq!(Balances::total_issuance(),issuance);assert_eq!(Balances::free_balance(crate::AiPenaltyDestination::get()),balance+400*DECIMALS);assert_eq!(pallet_staking::Ledger::<Runtime>::get(o).unwrap().active,9900*DECIMALS);assert_eq!(pallet_staking::Ledger::<Runtime>::get(n).unwrap().active,29700*DECIMALS);assert_eq!(Session::validators(),validators);}
 pipeline_cancel {let (b,g,o,n,issuance,balance)=consensus_fixture();}: {verified_reports(b,g,true);deferred_boundary();}
 verify {assert_eq!(Balances::total_issuance(),issuance);assert_eq!(Balances::free_balance(crate::AiPenaltyDestination::get()),balance);assert_eq!(pallet_staking::Ledger::<Runtime>::get(o).unwrap().active,10000*DECIMALS);assert_eq!(pallet_staking::Ledger::<Runtime>::get(n).unwrap().active,30000*DECIMALS);}
 pipeline_invalid {let (mut b,_,_,_,issuance,balance)=consensus_fixture();b.0.second_header=b.0.first_header.clone();}: {assert!(crate::Babe::report_equivocation_unsigned(RuntimeOrigin::none(),alloc::boxed::Box::new(b.0),b.1).is_err());}
 verify {assert!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).is_empty());assert_eq!(Balances::total_issuance(),issuance);assert_eq!(Balances::free_balance(crate::AiPenaltyDestination::get()),balance);}

 apply_oldest_span {setup(15,false);history(false);}: {apply_at(6)?;}
 verify {assert_eq!(pallet_staking::UnappliedSlashes::<Runtime>::get(25).len(),16);}
 apply_prune_spans {setup(15,false);history(true);}: {apply_at(156)?;}
 verify {assert_eq!(pallet_staking::UnappliedSlashes::<Runtime>::get(50).len(),16);assert_eq!(pallet_staking::SlashingSpans::<Runtime>::get(who(0)).unwrap().prior.len(),1);}
 apply_new_max {setup(15,false);}: {apply()?;}
 verify {assert_eq!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).len(),16);}
 apply_combined_cap {setup(16,true);}: {apply()?;}
 verify {assert_eq!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).len(),16);}
 apply_queue_rollback {setup(16,false);}: {assert!(apply().is_err());}
 verify {assert_eq!(pallet_staking::UnappliedSlashes::<Runtime>::get(24).len(),16);}
 apply_duplicate_identity {setup(16,false);}: {crate::v14_penalties::after_verified_report(who(0),0,1,16,false).map_err(|_|DispatchError::Other("duplicate"))?;}
 prepare_policy {setup(0,false);crate::v14_penalties::Policy::<Runtime>::kill();let session=Session::current_index()+1;}: {crate::EquivocationPenalties::prepare_policy(RuntimeOrigin::root(),10_000_000,session)?;}
}
