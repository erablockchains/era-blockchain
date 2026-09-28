//! Synthetic accounting faults are injected only in tests; no production state is imported.
use era_runtime::{fresh_genesis_inputs::Inputs, *};
use frame_support::{assert_ok, assert_noop, traits::{fungible::Mutate, tokens::{Preservation,Fortitude,Precision}}};
use sp_runtime::{BuildStorage,Perbill};
use pallet_security_budget as sb;
const INITIAL:u128=20_000_000*DECIMALS;
fn ext()->sp_io::TestExternalities {Inputs::synthetic().genesis().unwrap().build_storage().unwrap().into()}
fn points(era:u32,own:Option<u128>) {
 let validators=Session::validators();
 for v in &validators {
  let mut exposure=pallet_staking::ErasStakersOverview::<Runtime>::get(0,v).unwrap();
  if let Some(n)=own {exposure.own=n;exposure.total=n;}
  pallet_staking::ErasStakersOverview::<Runtime>::insert(era,v,exposure);
  pallet_staking::ErasValidatorPrefs::<Runtime>::insert(era,v,pallet_staking::ValidatorPrefs{commission:Perbill::zero(),blocked:false});
 }
 pallet_staking::ErasRewardPoints::<Runtime>::insert(era,pallet_staking::EraRewardPoints{total:400,individual:validators.into_iter().map(|v|(v,100)).collect()});
}
fn finalize(era:u32,duration:u64) {assert_ok!(SecurityBudget::finalize_completed_era(era,duration,1_000_000+(era as u64+1)*duration));}
fn conserved(){assert_eq!(SecurityBudget::unallocated_principal()+SecurityBudget::principal_reserved_total(),INITIAL);assert!(SecurityBudget::accounted_reward_total().unwrap()<=INITIAL);}
#[test]
fn principal_initialization_has_no_debt_and_replaces_minting() {ext().execute_with(||{
 assert_eq!(SecurityBudget::unallocated_principal(),INITIAL);assert_eq!(SecurityBudget::committed_liabilities(),0);
 let issuance=Balances::total_issuance();let allowance=IssuanceCap::remaining_allowance();let pot=SecurityBudget::staking_pot_balance();points(0,None);finalize(0,3_600_000);
 let b=SecurityBudget::era_budget(0).unwrap();assert!(b.total>0);assert_eq!(SecurityBudget::era_principal(0),b.total);assert_eq!(b.gross_issuance,0);assert_eq!(b.staking_issuance,0);assert_eq!(b.treasury_issuance,0);assert_eq!(Balances::total_issuance(),issuance);assert_eq!(IssuanceCap::remaining_allowance(),allowance);assert_eq!(SecurityBudget::staking_pot_balance(),pot);conserved();
});}
#[test]
fn ceiling_then_only_residual_target_is_minted_with_existing_split() {ext().execute_with(||{
 points(0,None);let allowance=IssuanceCap::remaining_allowance().unwrap();finalize(0,SecurityBudgetMillisecondsPerYear::get());
 let b=SecurityBudget::era_budget(0).unwrap();let principal=SecurityBudget::era_principal(0);assert_eq!(principal,SecurityBudget::PRINCIPAL_PER_ERA_CEILING);
 let target=SecurityBudget::target_staking_issuance(b.eligible_stake,b.duration_millis,0).unwrap().0;
 assert_eq!(principal+b.staking_issuance,target);assert_eq!(b.total,target);assert_eq!(b.staking_issuance,b.gross_issuance*9/10);assert_eq!(b.gross_issuance,b.staking_issuance+b.treasury_issuance);assert_eq!(allowance-IssuanceCap::remaining_allowance().unwrap(),b.gross_issuance);conserved();
});}
#[test]
fn missed_and_ineligible_eras_do_not_bank_extra_principal() {ext().execute_with(||{
 finalize(0,3_600_000);assert!(SecurityBudget::era_budget(0).is_none());assert_eq!(SecurityBudget::principal_reserved_total(),0);
 points(1,Some(DECIMALS));finalize(1,3_600_000);assert!(SecurityBudget::era_budget(1).is_none());assert_eq!(SecurityBudget::principal_reserved_total(),0);
 points(2,Some(10_000*DECIMALS));finalize(2,SecurityBudgetMillisecondsPerYear::get());assert_eq!(SecurityBudget::era_principal(2),SecurityBudget::PRINCIPAL_PER_ERA_CEILING);conserved();
});}
#[test]
fn liabilities_fees_tips_and_carry_are_not_misclassified_as_principal() {ext().execute_with(||{
 points(0,None);let pot=SecurityBudget::staking_pot_account();<Balances as Mutate<AccountId>>::set_balance(&pot,100*DECIMALS);
 sb::CommittedLiabilities::<Runtime>::put(96*DECIMALS);sb::UnallocatedFeeStaking::<Runtime>::put(DECIMALS);sb::UnallocatedTipFallback::<Runtime>::put(DECIMALS);sb::UnallocatedStakingCarry::<Runtime>::put(DECIMALS);issuance_cap::RemainingAllowance::<Runtime>::put(0);
 finalize(0,SecurityBudgetMillisecondsPerYear::get());let b=SecurityBudget::era_budget(0).unwrap();assert_eq!(SecurityBudget::era_principal(0),DECIMALS);assert_eq!(b.fee_staking,DECIMALS);assert_eq!(b.failed_author_tips,DECIMALS);assert_eq!(b.retained_carry,DECIMALS);assert_eq!(b.total,4*DECIMALS);assert_eq!(b.gross_issuance,0);assert_eq!(SecurityBudget::committed_liabilities(),100*DECIMALS);assert_eq!(IssuanceCap::remaining_allowance(),Some(0));conserved();
});}
#[test]
fn underfunded_existing_obligations_fail_without_reserving_principal() {ext().execute_with(||{
 points(0,None);sb::CommittedLiabilities::<Runtime>::put(INITIAL/2);
 <Balances as Mutate<AccountId>>::set_balance(&SecurityBudget::staking_pot_account(), DECIMALS);let before=SecurityBudget::unallocated_principal();
 assert_noop!(SecurityBudget::finalize_completed_era(0,3_600_000,4_600_000),sb::Error::<Runtime>::InsufficientStakingPot);assert_eq!(SecurityBudget::unallocated_principal(),before);assert_eq!(SecurityBudget::principal_reserved_total(),0);assert!(SecurityBudget::era_budget(0).is_none());
});}
#[test]
fn all_sources_share_exact_lifetime_budget_and_residue_is_not_swept() {ext().execute_with(||{
 points(0,None);sb::StakingPaidTotal::<Runtime>::put(INITIAL-7);sb::UnallocatedFeeStaking::<Runtime>::put(100);finalize(0,3_600_000);
 assert_eq!(SecurityBudget::era_principal(0),7);assert_eq!(SecurityBudget::era_budget(0).unwrap().total,7);assert_eq!(SecurityBudget::unallocated_fee_staking(),100);assert_eq!(SecurityBudget::remaining_reward_budget().unwrap(),0);
 points(1,None);finalize(1,3_600_000);assert!(SecurityBudget::era_budget(1).is_none());assert_eq!(SecurityBudget::unallocated_principal(),INITIAL-7);conserved();
});}
#[test]
fn exhausted_principal_falls_back_to_bounded_issuance() {ext().execute_with(||{
 sb::UnallocatedPrincipal::<Runtime>::put(0);sb::PrincipalReservedTotal::<Runtime>::put(INITIAL);points(0,None);finalize(0,3_600_000);let b=SecurityBudget::era_budget(0).unwrap();assert_eq!(SecurityBudget::era_principal(0),0);assert!(b.gross_issuance>0);assert_eq!(b.total,b.staking_issuance);conserved();
});}
#[test]
fn principal_ledger_remainder_bounds_last_tranche() {ext().execute_with(||{
 sb::UnallocatedPrincipal::<Runtime>::put(3);sb::PrincipalReservedTotal::<Runtime>::put(INITIAL-3);points(0,None);finalize(0,3_600_000);assert_eq!(SecurityBudget::era_principal(0),3);assert_eq!(SecurityBudget::unallocated_principal(),0);assert!(SecurityBudget::era_budget(0).unwrap().gross_issuance>0);conserved();
});}
#[test]
fn small_duration_rounding_carries_target_without_minting() {ext().execute_with(||{
 let mut total=0;let mut stake=0;
 for era in 0..17 {points(era,None);finalize(era,1);let b=SecurityBudget::era_budget(era).unwrap();stake=b.eligible_stake;total+=SecurityBudget::era_principal(era);assert_eq!(b.gross_issuance,0);}
 let (expected,remainder)=SecurityBudget::target_staking_issuance(stake,17,0).unwrap();assert_eq!(total,expected);assert_eq!(SecurityBudget::target_remainder(),remainder);conserved();
});}
#[test]
fn principal_claim_discharge_and_duplicate_claim_preserve_conservation() {ext().execute_with(||{
 points(0,None);finalize(0,3_600_000);let principal=SecurityBudget::principal_reserved_total();let pot=SecurityBudget::staking_pot_balance();let allowance=IssuanceCap::remaining_allowance();
 pallet_staking::ActiveEra::<Runtime>::put(pallet_staking::ActiveEraInfo{index:1,start:Some(4_600_000)});
 let caller=Inputs::synthetic().community;
 for validator in Session::validators(){assert_ok!(SecurityBudget::claim_reward_page(RuntimeOrigin::signed(caller.clone()),0,validator.clone(),0));assert_noop!(SecurityBudget::claim_reward_page(RuntimeOrigin::signed(caller.clone()),0,validator,0),sb::Error::<Runtime>::AlreadyClaimed);}
 assert_eq!(SecurityBudget::committed_liabilities(),0);assert_eq!(pot-SecurityBudget::staking_pot_balance(),SecurityBudget::staking_paid_total());assert_eq!(principal,SecurityBudget::staking_paid_total()+SecurityBudget::unallocated_staking_carry());assert_eq!(IssuanceCap::remaining_allowance(),allowance);conserved();
});}
#[test]
fn finalized_era_cannot_reserve_principal_twice() {ext().execute_with(||{
 points(0,None);finalize(0,3_600_000);let left=SecurityBudget::unallocated_principal();assert_noop!(SecurityBudget::finalize_completed_era(0,3_600_000,4_600_000),sb::Error::<Runtime>::EraAlreadyFinalized);assert_eq!(SecurityBudget::unallocated_principal(),left);conserved();
});}
