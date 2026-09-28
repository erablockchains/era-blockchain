//! Measurement definitions and synthetic branch exercises; no measured production weights.
//! Before exposure, measure each branch at the ranges below, both pair classes and both weight
//! dimensions. Account for bounded raw decoding, provider/account vector updates and rollback.
//! The real SDK custody, holds, donations and initial-floor cases live in runtime integration tests.
//! No registered FRAME benchmark or WeightInfo implementation is supplied at this dormant gate.
use frame_support::weights::Weight;

pub const POOLS: core::ops::RangeInclusive<u32> = 1..=1024;
pub const POSITIONS: core::ops::RangeInclusive<u32> = 0..=64;
pub const PROVIDERS: core::ops::RangeInclusive<u32> = 0..=1024;
pub const PAGE: core::ops::RangeInclusive<u32> = 1..=64;
pub const MEASUREMENT_BRANCHES: &[&str] = &[
    "create: empty/prefunded native and registered custody; foreign references/holds/locks/reservations",
    "create: identity/participant collision; provider overflow/no-acquisition; deposit/event rollback",
    "first add: U256 sqrt upper boundary; positive exit witness; one-unit counterexample; backend floors",
    "first add: preserved surplus supports liveness; foreign restriction rejects witness; rollback",
    "later add: both proportional branches; maximum account/provider vectors; LP/index updates",
    "remove: partial/reap/reinsert; locked-only residual; native/registered floors; preserved surplus",
    "swap exact in/out: both orientations; fee ceilings; U256 products and u128 overflow; invariant",
    "all mutations: direct donations unchanged; reserve deficit; both transfer failures; late event failure",
    "all mutations: malformed/oversize bounded state; reentrancy; whole transaction rollback",
    "queries: six v1 codecs; 64 records; exclusive cursor; malformed cursor/state; memory/byte maxima",
    "reconcile: provider bound plus one raw probe; malformed/orphan positions and index mismatch",
];
/// Future measured calls must satisfy BOTH D5 capacity and the runtime's applicable call limit.
/// This check grants no exposure and takes measured values, never a generated placeholder weight.
pub fn fits_exposure_limits(
    measured: Weight,
    normal_capacity: Weight,
    max_extrinsic: Weight,
) -> bool {
    let quarter = Weight::from_parts(
        normal_capacity.ref_time() / 4,
        normal_capacity.proof_size() / 4,
    );
    measured.all_lte(quarter) && measured.all_lte(max_extrinsic)
}
#[cfg(test)]
mod fixtures {
    use super::*;
    use crate::{mock::*, *};
    #[test]
    fn fixture_maximum_provider_index_and_page_are_exercised() {
        ext().execute_with(|| {
            let pair = initialized();
            for id in 2..=3 {
                Amm::add(&account(id), pair, [100, 100], [0, 0], 100).unwrap();
            }
            assert_eq!(Providers::<Test>::get(pair).len(), 3);
            assert_eq!(Amm::reconcile(pair), Ok(()));
            let before = root();
            assert_eq!(
                Amm::add(&account(4), pair, [100, 100], [0, 0], 100),
                Err(Fault::Bound)
            );
            assert_eq!(root(), before);
        });
    }

    #[test]
    fn amended_surplus_and_exit_branches_have_transactional_fixtures() {
        ext().execute_with(|| {
            let pair = (Asset::Native, Asset::Registered(1));
            let id = PoolId::new(pair.0.fungible().unwrap(), pair.1.fungible().unwrap()).unwrap();
            let who = Custody.derive_pool_account(&id).unwrap();
            seed(pair.0, &who, 51);
            seed(pair.1, &who, 71);
            Amm::create(&account(1), pair.0, pair.1, 100).unwrap();
            assert_eq!(Amm::surplus(pair).unwrap(), [51, 71]);
            let before = root();
            assert_eq!(
                Amm::add(&account(1), pair, [10_000, 1], [0, 0], 100),
                Err(amm::Error::InsufficientInitialLiquidity.into())
            );
            assert_eq!(root(), before);
            Amm::add(&account(1), pair, [10_000; 2], [0, 0], 100).unwrap();
            let quote = Amm::quote(pair, 100, false).unwrap();
            seed(pair.0, &who, 10_152);
            seed(pair.1, &who, 10_274);
            assert_eq!(Amm::quote(pair, 100, false).unwrap(), quote);
            Amm::swap(&account(2), pair, 100, quote.1, &account(3), 100, false).unwrap();
            assert_eq!(Amm::surplus(pair).unwrap(), [152, 274]);
            Amm::reconcile(pair).unwrap();
        });
    }
    #[test]
    fn exposure_weight_limits_check_each_dimension_and_both_independent_limits() {
        let capacity = Weight::from_parts(400, 800);
        let max = Weight::from_parts(90, 210);
        assert!(fits_exposure_limits(
            Weight::from_parts(90, 200),
            capacity,
            max
        ));
        assert!(!fits_exposure_limits(
            Weight::from_parts(91, 200),
            capacity,
            max
        ));
        assert!(!fits_exposure_limits(
            Weight::from_parts(90, 201),
            capacity,
            max
        ));
        assert!(!fits_exposure_limits(
            Weight::from_parts(101, 100),
            capacity,
            Weight::MAX
        ));
        assert!(!fits_exposure_limits(
            Weight::from_parts(10, 100),
            capacity,
            Weight::from_parts(90, 99)
        ));
    }
}
