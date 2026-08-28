//! Temporary conservative weights for the Step 1A scaffold.
//!
//! RELEASE_BLOCKER: replace these constants with generated benchmark output before release.

use frame_support::weights::Weight;

pub trait WeightInfo {
    fn register_world() -> Weight;
    fn update_commitment() -> Weight;
    fn deregister_world() -> Weight;
    fn admin_remove_world() -> Weight;
    fn pause() -> Weight;
    fn unpause() -> Weight;
}

impl WeightInfo for () {
    fn register_world() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn update_commitment() -> Weight {
        Weight::from_parts(75_000_000, 8_000)
    }
    fn deregister_world() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn admin_remove_world() -> Weight {
        Weight::from_parts(100_000_000, 10_000)
    }
    fn pause() -> Weight {
        Weight::from_parts(25_000_000, 4_000)
    }
    fn unpause() -> Weight {
        Weight::from_parts(25_000_000, 4_000)
    }
}
