//! Ordered, transactional V13 retained-supply reconciliation.
//!
//! This is a concrete one-shot runtime migration. It consumes only the four custody identities
//! left symbolic by the approved vector; all source accounts, amounts and invariants are compiled
//! into the runtime.

use alloc::vec::Vec;

#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
use frame_support::{
    dispatch::DispatchResult,
    ensure,
    storage::{with_transaction, TransactionOutcome},
    traits::{
        fungible::{Inspect, Mutate},
        tokens::{Fortitude, Precision, Preservation},
        OnRuntimeUpgrade, PalletInfoAccess, StorageVersion, VestingSchedule,
    },
    weights::{constants::RocksDbWeight, Weight},
};
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

use crate::{
    issuance_cap::{
        self, RemainingAllowance, V13Completion, V13MigrationCompleted, V13MigrationInput,
    },
    upgrade13_policy::{
        provisional_c2_two_schedule_terms, ACTIVE_PRESALE_POOL, AIRDROP_VALIDATOR_POOL,
        ECOSYSTEM_POOL, FOUNDING_ALLOCATION_TARGETS, FOUNDING_MEMBERS_POOL, LIQUIDITY_RESERVE_POOL,
        MAXIMUM_POST_CORRECTION_NEW_ISSUANCE, REWARD_RESERVE_GROSS_TARGET,
        REWARD_RESERVE_SPENDABLE_TARGET, SUDO_OPERATIONAL_RESERVE, SUDO_TARGET_FREE,
        TARGET_RETAINED_ISSUANCE,
    },
    AccountId, Assets, Balance, Balances, BlockNumber, EraWorlds, IssuanceCap, Multisig, Nfts,
    Proxy, RewardReserve, Runtime, RuntimeHoldReason, Staking, System, Vesting,
};

pub const MIGRATION_VERSION: u16 = 13;
pub const R6R3_PRE_MIGRATION_ISSUANCE: Balance = 1_005_003_999_999_999_994_375_836_000;
pub const REQUIRED_DESTRUCTION: Balance = 905_003_999_999_999_994_375_836_000;
pub const MAX_REWARD_LIABILITY_ENTRIES: u32 = 512;

/// 0.75 seconds plus conservative database work and a 3.5 MiB proof-size budget.
///
/// The database bound covers 640 reads (including the maximum liability scan and repeated
/// postcondition reads) and 160 writes (account/issuance changes, ten vesting schedules, locks,
/// input removal, allowance, version and marker). The snapshot proof must confirm actual weight
/// remains below this bound.
pub fn declared_weight() -> Weight {
    Weight::from_parts(750_000_000_000, 3_670_016)
        .saturating_add(RocksDbWeight::get().reads_writes(640, 160))
}

const fn nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => 0xff,
    }
}

const fn decode_hex(value: &[u8; 64]) -> [u8; 32] {
    let mut result = [0u8; 32];
    let mut index = 0;
    while index < 32 {
        let high = nibble(value[index * 2]);
        let low = nibble(value[index * 2 + 1]);
        assert!(high != 0xff && low != 0xff, "invalid compiled account hex");
        result[index] = high * 16 + low;
        index += 1;
    }
    result
}

const APPROVED_VECTOR_HASH: [u8; 32] =
    decode_hex(b"12907ee338111ccb170a00cd37b5da1b823cbc47f4b110c2ff02fa2918110b09");

#[derive(Clone, Copy)]
struct ExpectedBalance {
    account: [u8; 32],
    amount: Balance,
}

const fn expected(account: &[u8; 64], amount: Balance) -> ExpectedBalance {
    ExpectedBalance {
        account: decode_hex(account),
        amount,
    }
}

#[derive(Clone, Copy)]
struct BurnPlan {
    account: [u8; 32],
    current: Balance,
    destruction: Balance,
    target: Balance,
}

const fn burn_plan(
    account: &[u8; 64],
    current: Balance,
    destruction: Balance,
    target: Balance,
) -> BurnPlan {
    BurnPlan {
        account: decode_hex(account),
        current,
        destruction,
        target,
    }
}

const FOUNDERS: [BurnPlan; 5] = [
    burn_plan(
        b"ba29ecf97ed52e7fcc0fdd2cc364a646e140bd8c79862a57db06db685d03567c",
        80_000_000_000_000_000_000_000_000,
        72_000_000_000_000_000_000_000_000,
        8_000_000_000_000_000_000_000_000,
    ),
    burn_plan(
        b"aa0254cda2a14ab6c8ef9db2df74d48a175a7504f8a16775c0296975149e3d4a",
        30_000_000_000_000_000_000_000_000,
        25_000_000_000_000_000_000_000_000,
        5_000_000_000_000_000_000_000_000,
    ),
    burn_plan(
        b"a4520fa060b013a5e4430f18ce8ea5568db96d000594a5857b212a5f3f631d6d",
        30_000_000_000_000_000_000_000_000,
        27_000_000_000_000_000_000_000_000,
        3_000_000_000_000_000_000_000_000,
    ),
    burn_plan(
        b"981bb1c87263351dded88df780557dc8ae5f96aa7f7bcbccda8ea9ab8424dd3b",
        30_000_000_000_000_000_000_000_000,
        28_000_000_000_000_000_000_000_000,
        2_000_000_000_000_000_000_000_000,
    ),
    burn_plan(
        b"b43d1c7b0f4e2d3a343c49699d3ba5060e06299a66e91f3542ef441ce2dc4306",
        30_000_000_000_000_000_000_000_000,
        28_000_000_000_000_000_000_000_000,
        2_000_000_000_000_000_000_000_000,
    ),
];

const SUDO: BurnPlan = burn_plan(
    b"da351e29c2979fdb6ec907bb55dee76300e733f39e7debe5f2d929e8abdde02b",
    1_000_000_000_000_000_000_000_000,
    999_998_999_900_000_000_000_000,
    SUDO_TARGET_FREE,
);
const REWARD_POT: BurnPlan = burn_plan(
    b"6d6f646c6572612f727764730000000000000000000000000000000000000000",
    30_000_000_000_100_000_000_000_000,
    10_000_000_000_100_000_000_000_000,
    REWARD_RESERVE_GROSS_TARGET,
);
const PRESALE_SOURCE: BurnPlan = burn_plan(
    b"6ad9a525f201b0ede03dbf123f49efcf648f8ec126f323972a399612c5adaa17",
    200_000_000_000_000_000_000_000_000,
    180_000_000_000_000_000_000_000_000,
    0,
);
const ECOSYSTEM_SOURCE: BurnPlan = burn_plan(
    b"b0e92b4a5aecd4d080457e2376b1d6b55ded11f823b79bcda497f2cb272de262",
    119_999_999_999_699_999_675_529_000,
    100_000_000_999_999_999_675_529_000,
    0,
);
const LIQUIDITY_SOURCE: BurnPlan = burn_plan(
    b"f2ce653391f33f156fd6c8dab9492d82976e7b0fbbd6320d5d495ee6ade0fe35",
    150_000_000_000_000_000_000_000_000,
    140_000_000_000_000_000_000_000_000,
    0,
);
const COMMUNITY_SOURCE: BurnPlan = burn_plan(
    b"62954029a1b33bb722fa20fab2edca9e17e401ab98e51cbf73c1b24bbf836349",
    299_981_952_999_999_997_728_703_000,
    294_003_999_999_999_994_700_307_000,
    0,
);

const COMMUNITY_STAKING_TOTAL: Balance = 270_000_000_000_000_000_000;
const COMMUNITY_STAKING_ACTIVE: Balance = 220_000_000_000_000_000_000;
const COMMUNITY_STAKING_UNLOCKING: Balance = 50_000_000_000_000_000_000;
const COMMUNITY_STAKING_UNLOCK_ERA: sp_staking::EraIndex = 45;
const COMMUNITY_NOMINATION_SUBMITTED_IN: sp_staking::EraIndex = 887;
const COMMUNITY_NOMINATION_TARGET: [u8; 32] =
    decode_hex(b"883bca52dd9af73e19b57e6c5094e0ce5fa261720ebcb62d652012081167785d");

const PRESALE_TRANSFER: Balance = 20_000_000_000_000_000_000_000_000;
const PRE_RESERVE_ECOSYSTEM_TRANSFER: Balance = 19_999_999_999_700_000_000_000_000;
const ECOSYSTEM_TRANSFER: Balance = PRE_RESERVE_ECOSYSTEM_TRANSFER - SUDO_OPERATIONAL_RESERVE;
const LIQUIDITY_TRANSFER: Balance = 10_000_000_000_000_000_000_000_000;
const COMMUNITY_TRANSFER: Balance = 5_977_953_000_000_003_028_396_000;

const AIRDROP_PRESERVED: [ExpectedBalance; 22] = [
    expected(
        b"beb8a9f84b72fcda876de7b72b303933e2c8dac34fe6e7ddeded19d8116e0203",
        1_000_000_000_000_000_000_000_000,
    ),
    expected(
        b"5222a5e0d2ab594bf5094cf13cc2f64107fb8ee60139c678ccaa4f76d9eb9112",
        707_999_999_999_351_058_000,
    ),
    expected(
        b"da43658faf9b0c49482766121dbce9254e24abb8dbaf365f2cb073c87b972e1a",
        315_999_999_998_593_959_000,
    ),
    expected(
        b"4c8ad8df1f2d7e4499b3be3c6ca4dc549ea1a8828a72e2a6978147be884ebb0f",
        100_000_000_000_000_000_000,
    ),
    expected(
        b"0297780a7c2a40a632efc84bd2a6d02274778f0e2b6e27b18ee7f8759afc2f74",
        10_000_000_000_000_000_000_000,
    ),
    expected(
        b"00c6293e512769fe8097009c7e6c1d20db6516dedb60b12c6c86106bfcae8605",
        12_000_000_000_000_000_000,
    ),
    expected(
        b"7ae96ce3bf65601f4272592ea3caf16347d035551207ed5492400bf4bf695e1a",
        389_999_999_999_675_529_000,
    ),
    expected(
        b"1023876d35c4bf0715209cb7dd9895b8d87086915acb1041af5a58c1c649184f",
        139_999_999_999_675_529_000,
    ),
    expected(
        b"4236a6fbea2f7399d43d7256332161f8e34d6658cac92315791a70c5c3f1b73f",
        61_000_000_000_000_000_000,
    ),
    expected(
        b"4cb817b8619f4961f26655b0a661e0abf894f110fce94f9f2b8433ad01273538",
        10_000_000_000_000_000_000,
    ),
    expected(
        b"940728b329b59ba7509ffe34c75ff5eb8351161b754f43e9080066bd9b97791c",
        10_000_000_000_000_000_000,
    ),
    expected(
        b"14f6f27e974f53fba74ffff528a7731aaacf30301ace603dd4dd0df2b5be3729",
        1_000_000_000_000_000_000_000,
    ),
    expected(
        b"a22eb31fe154146d94439c92bc6ac379103e2788b1b7d7b1f1672a07928d047e",
        249_999_999_999_783_686_000,
    ),
    expected(
        b"784ff134f16f1796f8eb0ecb12e35161f5fbe64119fd3e4b93687a32b3046502",
        5_000_000_000_000_000_000_000,
    ),
    expected(
        b"1afc5525abadf766865a1155f7646b1c21718508670b888942cda0e4518e1a23",
        49_999_999_999_891_843_000,
    ),
    expected(
        b"7e11ab0b782b0f3b69ad99cec2b0568b203185574cc3006cd8305de1f3e46e71",
        1_000_000_000_000_000_000_000_000,
    ),
    expected(
        b"f8bd8c22125cc6e522b667d572140fa38fa2400769c750e0f19ceba69e581205",
        1_000_000_000_000_000_000_000_000,
    ),
    expected(
        b"883bca52dd9af73e19b57e6c5094e0ce5fa261720ebcb62d652012081167785d",
        1_000_000_000_000_000_000_000_000,
    ),
    expected(
        b"36d8b1ab76fcc5c21377456309e337f69e6c7e6aee4c8605defc8b44ff093a2d",
        1_000_000_000_000_000_000_000,
    ),
    expected(
        b"4e556c16ef96ab1d557c284b666b97e47c64651ca709e3112b3ec60e00684111",
        1_000_000_000_000_000_000_000,
    ),
    expected(
        b"b48f671ff9a8774873df0e2cf212d2b6ea8192766802032e8ed791ae447d863b",
        1_000_000_000_000_000_000_000,
    ),
    expected(
        b"78892e69abca4085d4cceb34716f03ee88619418f1f81e43b772a8df832f997c",
        1_000_000_000_000_000_000_000,
    ),
];

const ECOSYSTEM_PRESERVED: [ExpectedBalance; 2] = [
    expected(
        b"6d6f646c6572612f666565730000000000000000000000000000000000000000",
        100_000_000_000_000,
    ),
    expected(
        b"6d6f646c6572612f747265610000000000000000000000000000000000000000",
        100_000_000_000_000,
    ),
];

#[cfg(test)]
pub(crate) fn test_input() -> V13MigrationInput<AccountId> {
    V13MigrationInput {
        reconciliation_vector_hash: APPROVED_VECTOR_HASH,
        active_presale_custody: AccountId::new([0xf0; 32]),
        ecosystem_custody: AccountId::new([0xf1; 32]),
        liquidity_custody: AccountId::new([0xf2; 32]),
        community_onboarding_custody: AccountId::new([0xf3; 32]),
    }
}

#[cfg(test)]
pub(crate) fn test_pre_migration_balances() -> Vec<(AccountId, Balance)> {
    let mut balances = Vec::with_capacity(35);
    balances.extend(
        FOUNDERS
            .iter()
            .map(|plan| (account(plan.account), plan.current)),
    );
    balances.extend([
        (account(SUDO.account), SUDO.current),
        (account(REWARD_POT.account), REWARD_POT.current),
        (account(PRESALE_SOURCE.account), PRESALE_SOURCE.current),
        (account(ECOSYSTEM_SOURCE.account), ECOSYSTEM_SOURCE.current),
        (account(LIQUIDITY_SOURCE.account), LIQUIDITY_SOURCE.current),
        (account(COMMUNITY_SOURCE.account), COMMUNITY_SOURCE.current),
    ]);
    balances.extend(
        AIRDROP_PRESERVED
            .iter()
            .chain(ECOSYSTEM_PRESERVED.iter())
            .map(|item| (account(item.account), item.amount)),
    );
    balances
}

#[cfg(test)]
pub(crate) fn test_community_source() -> AccountId {
    account(COMMUNITY_SOURCE.account)
}

#[cfg(test)]
pub(crate) fn test_sudo_account() -> AccountId {
    account(SUDO.account)
}

#[cfg(test)]
pub(crate) fn test_ecosystem_preserved_accounts() -> [AccountId; 2] {
    ECOSYSTEM_PRESERVED.map(|item| account(item.account))
}

#[cfg(test)]
pub(crate) fn test_initialize_community_staking() {
    let who = test_community_source();
    Staking::bond(
        crate::RuntimeOrigin::signed(who.clone()),
        COMMUNITY_STAKING_TOTAL,
        pallet_staking::RewardDestination::Staked,
    )
    .expect("exact R6R3 community staking bond fixture");
    pallet_staking::Ledger::<Runtime>::mutate(&who, |stored| {
        let ledger = stored.as_mut().expect("bond created staking ledger");
        ledger.active = COMMUNITY_STAKING_ACTIVE;
        ledger
            .unlocking
            .try_push(pallet_staking::UnlockChunk {
                value: COMMUNITY_STAKING_UNLOCKING,
                era: COMMUNITY_STAKING_UNLOCK_ERA,
            })
            .expect("one unlocking chunk fits");
    });
    Staking::do_add_nominator(
        &who,
        pallet_staking::Nominations {
            targets: vec![account(COMMUNITY_NOMINATION_TARGET)]
                .try_into()
                .expect("one nomination target fits"),
            submitted_in: COMMUNITY_NOMINATION_SUBMITTED_IN,
            suppressed: false,
        },
    );
    validate_community_staking_pending().expect("exact R6R3 community staking fixture");
}

fn account(bytes: [u8; 32]) -> AccountId {
    AccountId::new(bytes)
}

fn checked_sum(values: impl IntoIterator<Item = Balance>) -> Result<Balance, DispatchError> {
    values.into_iter().try_fold(0u128, |sum, value| {
        sum.checked_add(value)
            .ok_or(DispatchError::Other("V13 sum overflow"))
    })
}

fn balance(who: &AccountId) -> Balance {
    <Balances as Inspect<AccountId>>::total_balance(who)
}

fn validate_community_staking_pending() -> DispatchResult {
    let who = account(COMMUNITY_SOURCE.account);
    let free = <Balances as Inspect<AccountId>>::balance(&who);
    ensure!(
        free.checked_add(COMMUNITY_STAKING_TOTAL) == Some(COMMUNITY_SOURCE.current),
        DispatchError::Other("V13 community free/reserved balance mismatch")
    );
    let account_info = frame_system::Account::<Runtime>::get(&who);
    ensure!(
        account_info.data.reserved == COMMUNITY_STAKING_TOTAL && account_info.data.frozen == 0,
        DispatchError::Other("V13 community account data mismatch")
    );
    let holds = pallet_balances::Holds::<Runtime>::get(&who);
    ensure!(
        holds.len() == 1
            && holds[0].id == RuntimeHoldReason::Staking(pallet_staking::HoldReason::Staking)
            && holds[0].amount == COMMUNITY_STAKING_TOTAL,
        DispatchError::Other("V13 community staking hold mismatch")
    );
    ensure!(
        pallet_balances::Locks::<Runtime>::get(&who).is_empty()
            && pallet_balances::Freezes::<Runtime>::get(&who).is_empty(),
        DispatchError::Other("V13 community unexpected balance restriction")
    );

    ensure!(
        pallet_staking::Bonded::<Runtime>::get(&who) == Some(who.clone()),
        DispatchError::Other("V13 community bonded pairing mismatch")
    );
    let ledger = pallet_staking::Ledger::<Runtime>::get(&who)
        .ok_or(DispatchError::Other("V13 community staking ledger absent"))?;
    ensure!(
        ledger.stash == who
            && ledger.total == COMMUNITY_STAKING_TOTAL
            && ledger.active == COMMUNITY_STAKING_ACTIVE
            && ledger.unlocking.len() == 1
            && ledger.unlocking[0].value == COMMUNITY_STAKING_UNLOCKING
            && ledger.unlocking[0].era == COMMUNITY_STAKING_UNLOCK_ERA
            && ledger.legacy_claimed_rewards.is_empty()
            && ledger.controller.is_none(),
        DispatchError::Other("V13 community staking ledger mismatch")
    );
    let nominations = pallet_staking::Nominators::<Runtime>::get(&who)
        .ok_or(DispatchError::Other("V13 community nominations absent"))?;
    ensure!(
        nominations.targets.as_slice() == [account(COMMUNITY_NOMINATION_TARGET)]
            && nominations.submitted_in == COMMUNITY_NOMINATION_SUBMITTED_IN
            && !nominations.suppressed,
        DispatchError::Other("V13 community nominations mismatch")
    );
    ensure!(
        pallet_staking::Payee::<Runtime>::get(&who)
            == Some(pallet_staking::RewardDestination::Staked)
            && pallet_staking::SlashingSpans::<Runtime>::get(&who).is_none(),
        DispatchError::Other("V13 community staking metadata mismatch")
    );
    Ok(())
}

fn retire_community_staking() -> DispatchResult {
    let who = account(COMMUNITY_SOURCE.account);
    Staking::force_unstake(frame_system::RawOrigin::Root.into(), who.clone(), 0)?;
    ensure!(
        pallet_staking::Bonded::<Runtime>::get(&who).is_none()
            && pallet_staking::Ledger::<Runtime>::get(&who).is_none()
            && pallet_staking::Nominators::<Runtime>::get(&who).is_none()
            && pallet_staking::Payee::<Runtime>::get(&who).is_none()
            && pallet_balances::Holds::<Runtime>::get(&who).is_empty()
            && frame_system::Account::<Runtime>::get(&who).data.reserved == 0
            && <Balances as Inspect<AccountId>>::balance(&who) == COMMUNITY_SOURCE.current,
        DispatchError::Other("V13 community staking cleanup mismatch")
    );
    Ok(())
}

fn validate_community_staking_completed() -> DispatchResult {
    let who = account(COMMUNITY_SOURCE.account);
    ensure!(
        pallet_staking::Bonded::<Runtime>::get(&who).is_none()
            && pallet_staking::Ledger::<Runtime>::get(&who).is_none()
            && pallet_staking::Nominators::<Runtime>::get(&who).is_none()
            && pallet_staking::Payee::<Runtime>::get(&who).is_none()
            && pallet_balances::Holds::<Runtime>::get(&who).is_empty(),
        DispatchError::Other("V13 community staking state remains")
    );
    Ok(())
}

fn ensure_balance(expected: ExpectedBalance) -> DispatchResult {
    ensure!(
        balance(&account(expected.account)) == expected.amount,
        DispatchError::Other("V13 account balance mismatch")
    );
    Ok(())
}

fn ensure_burn_plan_before(plan: BurnPlan) -> DispatchResult {
    ensure!(
        balance(&account(plan.account)) == plan.current,
        DispatchError::Other("V13 source balance mismatch")
    );
    ensure!(
        plan.destruction
            .checked_add(plan.target)
            .ok_or(DispatchError::Other("V13 source arithmetic overflow"))?
            <= plan.current,
        DispatchError::Other("V13 invalid destruction plan")
    );
    Ok(())
}

fn ensure_burn_plan_after(plan: BurnPlan) -> DispatchResult {
    ensure!(
        balance(&account(plan.account)) == plan.target,
        DispatchError::Other("V13 post-reconciliation balance mismatch")
    );
    Ok(())
}

fn reward_liability_summary() -> Result<(u32, Balance, [u8; 32]), DispatchError> {
    let mut count = 0u32;
    let mut total = 0u128;
    #[cfg(feature = "try-runtime")]
    let mut digest = [0u8; 32];
    #[cfg(not(feature = "try-runtime"))]
    let digest = [0u8; 32];
    for (era, liability) in pallet_reward_reserve::EraRewardLiabilities::<Runtime>::iter() {
        count = count
            .checked_add(1)
            .ok_or(DispatchError::Other("V13 liability count overflow"))?;
        ensure!(
            count <= MAX_REWARD_LIABILITY_ENTRIES,
            DispatchError::Other("V13 liability scan exceeds bound")
        );
        total = total
            .checked_add(liability)
            .ok_or(DispatchError::Other("V13 liability sum overflow"))?;
        #[cfg(feature = "try-runtime")]
        {
            digest = sp_io::hashing::blake2_256(&(digest, era, liability).encode());
        }
        #[cfg(not(feature = "try-runtime"))]
        let _ = era;
    }
    Ok((count, total, digest))
}

fn validate_reward_liabilities() -> DispatchResult {
    let (_, summed, _) = reward_liability_summary()?;
    let committed = pallet_reward_reserve::CommittedLiabilities::<Runtime>::get();
    ensure!(
        summed == committed,
        DispatchError::Other("V13 earned reward liabilities mismatch")
    );
    ensure!(
        committed <= REWARD_RESERVE_SPENDABLE_TARGET,
        DispatchError::Other("V13 reward reserve insolvent")
    );
    Ok(())
}

fn prefix_contains_only_version<P: PalletInfoAccess>(expected: StorageVersion) -> bool {
    if StorageVersion::get::<P>() != expected {
        return false;
    }
    let prefix = P::name_hash();
    let version_key = StorageVersion::storage_key::<P>();
    let mut cursor = prefix.to_vec();
    let mut seen = false;
    loop {
        let Some(key) = sp_io::storage::next_key(&cursor) else {
            break;
        };
        if !key.starts_with(&prefix) {
            break;
        }
        if key.as_slice() != version_key || seen {
            return false;
        }
        seen = true;
        cursor = key;
    }
    seen
}

fn validate_application_state() -> DispatchResult {
    ensure!(
        prefix_contains_only_version::<Multisig>(StorageVersion::new(1)),
        DispatchError::Other("V13 Multisig state/version mismatch")
    );
    ensure!(
        prefix_contains_only_version::<Proxy>(StorageVersion::new(0)),
        DispatchError::Other("V13 Proxy state/version mismatch")
    );
    ensure!(
        prefix_contains_only_version::<Assets>(StorageVersion::new(1)),
        DispatchError::Other("V13 Assets state/version mismatch")
    );
    ensure!(
        prefix_contains_only_version::<Nfts>(StorageVersion::new(1)),
        DispatchError::Other("V13 Nfts state/version mismatch")
    );
    ensure!(
        prefix_contains_only_version::<EraWorlds>(StorageVersion::new(1)),
        DispatchError::Other("V13 EraWorlds state/version mismatch")
    );
    Ok(())
}

fn validate_versions() -> DispatchResult {
    ensure!(
        StorageVersion::get::<IssuanceCap>() == StorageVersion::new(1),
        DispatchError::Other("V13 IssuanceCap storage version is not 1")
    );
    ensure!(
        StorageVersion::get::<RewardReserve>() == StorageVersion::new(2),
        DispatchError::Other("V13 RewardReserve storage version is not 2")
    );
    validate_application_state()
}

fn all_known_accounts() -> Vec<AccountId> {
    let mut result = Vec::with_capacity(35);
    result.extend(FOUNDERS.iter().map(|plan| account(plan.account)));
    result.extend([
        account(SUDO.account),
        account(REWARD_POT.account),
        account(PRESALE_SOURCE.account),
        account(ECOSYSTEM_SOURCE.account),
        account(LIQUIDITY_SOURCE.account),
        account(COMMUNITY_SOURCE.account),
    ]);
    result.extend(AIRDROP_PRESERVED.iter().map(|item| account(item.account)));
    result.extend(ECOSYSTEM_PRESERVED.iter().map(|item| account(item.account)));
    result
}

fn validate_input(input: &V13MigrationInput<AccountId>) -> DispatchResult {
    ensure!(
        input.reconciliation_vector_hash == APPROVED_VECTOR_HASH,
        DispatchError::Other("V13 reconciliation vector hash mismatch")
    );
    let destinations = [
        &input.active_presale_custody,
        &input.ecosystem_custody,
        &input.liquidity_custody,
        &input.community_onboarding_custody,
    ];
    for (index, destination) in destinations.iter().enumerate() {
        ensure!(
            balance(destination) == 0,
            DispatchError::Other("V13 custody destination is not empty")
        );
        ensure!(
            !frame_system::Account::<Runtime>::contains_key(destination),
            DispatchError::Other("V13 custody destination already has system state")
        );
        ensure!(
            !all_known_accounts()
                .iter()
                .any(|known| known == *destination),
            DispatchError::Other("V13 custody destination aliases a vector account")
        );
        ensure!(
            destinations[..index]
                .iter()
                .all(|earlier| *earlier != *destination),
            DispatchError::Other("V13 custody destinations are not distinct")
        );
    }
    Ok(())
}

fn validate_policy_arithmetic() -> DispatchResult {
    let destruction = checked_sum(FOUNDERS.iter().map(|plan| plan.destruction).chain([
        SUDO.destruction,
        REWARD_POT.destruction,
        PRESALE_SOURCE.destruction,
        ECOSYSTEM_SOURCE.destruction,
        LIQUIDITY_SOURCE.destruction,
        COMMUNITY_SOURCE.destruction,
    ]))?;
    ensure!(
        destruction == REQUIRED_DESTRUCTION,
        DispatchError::Other("V13 destruction total mismatch")
    );
    ensure!(
        R6R3_PRE_MIGRATION_ISSUANCE.checked_sub(destruction) == Some(TARGET_RETAINED_ISSUANCE),
        DispatchError::Other("V13 issuance arithmetic mismatch")
    );
    ensure!(
        checked_sum(FOUNDING_ALLOCATION_TARGETS)? == FOUNDING_MEMBERS_POOL,
        DispatchError::Other("V13 founding pool mismatch")
    );
    ensure!(
        PRESALE_TRANSFER == ACTIVE_PRESALE_POOL && LIQUIDITY_TRANSFER == LIQUIDITY_RESERVE_POOL,
        DispatchError::Other("V13 transfer pool mismatch")
    );
    ensure!(
        ECOSYSTEM_TRANSFER.checked_add(SUDO_OPERATIONAL_RESERVE)
            == Some(PRE_RESERVE_ECOSYSTEM_TRANSFER)
            && SUDO.target == crate::EXISTENTIAL_DEPOSIT + SUDO_OPERATIONAL_RESERVE,
        DispatchError::Other("V13 Sudo reserve accounting mismatch")
    );
    Ok(())
}

fn validate_pending(input: &V13MigrationInput<AccountId>) -> DispatchResult {
    validate_policy_arithmetic()?;
    validate_versions()?;
    ensure!(
        <Balances as Inspect<AccountId>>::total_issuance() == R6R3_PRE_MIGRATION_ISSUANCE,
        DispatchError::Other("V13 pre-migration issuance mismatch")
    );
    ensure!(
        RemainingAllowance::<Runtime>::get().is_none(),
        DispatchError::Other("V13 allowance is partially initialized")
    );
    ensure!(
        V13MigrationCompleted::<Runtime>::get().is_none(),
        DispatchError::Other("V13 completion marker unexpectedly present")
    );
    validate_input(input)?;

    for plan in FOUNDERS {
        ensure_burn_plan_before(plan)?;
        let who = account(plan.account);
        ensure!(
            pallet_vesting::Vesting::<Runtime>::get(&who).is_none(),
            DispatchError::Other("V13 founder already has vesting state")
        );
        ensure!(
            pallet_balances::Locks::<Runtime>::get(&who).is_empty(),
            DispatchError::Other("V13 founder already has balance locks")
        );
    }
    for plan in [
        SUDO,
        REWARD_POT,
        PRESALE_SOURCE,
        ECOSYSTEM_SOURCE,
        LIQUIDITY_SOURCE,
        COMMUNITY_SOURCE,
    ] {
        ensure_burn_plan_before(plan)?;
    }
    for item in AIRDROP_PRESERVED {
        ensure_balance(item)?;
    }
    for item in ECOSYSTEM_PRESERVED {
        ensure_balance(item)?;
    }
    validate_community_staking_pending()?;
    validate_reward_liabilities()
}

fn burn_exact(plan: BurnPlan) -> DispatchResult {
    let burned = <Balances as Mutate<AccountId>>::burn_from(
        &account(plan.account),
        plan.destruction,
        Preservation::Expendable,
        Precision::Exact,
        Fortitude::Force,
    )?;
    ensure!(
        burned == plan.destruction,
        DispatchError::Other("V13 burn was not exact")
    );
    Ok(())
}

fn transfer_exact(source: BurnPlan, destination: &AccountId, amount: Balance) -> DispatchResult {
    let transferred = <Balances as Mutate<AccountId>>::transfer(
        &account(source.account),
        destination,
        amount,
        Preservation::Expendable,
    )?;
    ensure!(
        transferred == amount,
        DispatchError::Other("V13 custody transfer was not exact")
    );
    Ok(())
}

fn add_founding_vesting(completed_at: BlockNumber) -> DispatchResult {
    for (plan, target) in FOUNDERS.iter().zip(FOUNDING_ALLOCATION_TARGETS) {
        ensure!(
            plan.target == target,
            DispatchError::Other("V13 founding target order mismatch")
        );
        let who = account(plan.account);
        let pair = provisional_c2_two_schedule_terms(target, completed_at)
            .map_err(|_| DispatchError::Other("V13 vesting arithmetic failed"))?;
        <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
            &who,
            pair.schedule_a.locked,
            pair.schedule_a.per_block,
            pair.schedule_a.starting_block,
        )?;
        <Vesting as VestingSchedule<AccountId>>::add_vesting_schedule(
            &who,
            pair.schedule_b.locked,
            pair.schedule_b.per_block,
            pair.schedule_b.starting_block,
        )?;
    }
    Ok(())
}

fn validate_founding_vesting(completed_at: BlockNumber) -> DispatchResult {
    for (plan, target) in FOUNDERS.iter().zip(FOUNDING_ALLOCATION_TARGETS) {
        let who = account(plan.account);
        let schedules = pallet_vesting::Vesting::<Runtime>::get(&who)
            .ok_or(DispatchError::Other("V13 founding vesting missing"))?;
        ensure!(
            schedules.len() == 2,
            DispatchError::Other("V13 founding vesting schedule count mismatch")
        );
        let pair = provisional_c2_two_schedule_terms(target, completed_at)
            .map_err(|_| DispatchError::Other("V13 vesting postcondition arithmetic failed"))?;
        let expected = [pair.schedule_a, pair.schedule_b];
        for (actual, expected) in schedules.iter().zip(expected) {
            ensure!(
                actual.locked() == expected.locked
                    && actual.per_block() == expected.per_block
                    && actual.starting_block() == expected.starting_block,
                DispatchError::Other("V13 founding vesting schedule mismatch")
            );
        }
    }
    Ok(())
}

fn validate_final_allocations(marker: &V13Completion<AccountId, BlockNumber>) -> DispatchResult {
    for plan in FOUNDERS {
        ensure_burn_plan_after(plan)?;
    }
    for plan in [
        SUDO,
        REWARD_POT,
        PRESALE_SOURCE,
        ECOSYSTEM_SOURCE,
        LIQUIDITY_SOURCE,
        COMMUNITY_SOURCE,
    ] {
        ensure_burn_plan_after(plan)?;
    }
    for item in AIRDROP_PRESERVED {
        ensure_balance(item)?;
    }
    for item in ECOSYSTEM_PRESERVED {
        ensure_balance(item)?;
    }
    ensure!(
        balance(&marker.input.active_presale_custody) == PRESALE_TRANSFER,
        DispatchError::Other("V13 presale custody allocation mismatch")
    );
    ensure!(
        balance(&marker.input.ecosystem_custody) == ECOSYSTEM_TRANSFER,
        DispatchError::Other("V13 ecosystem custody allocation mismatch")
    );
    ensure!(
        balance(&marker.input.liquidity_custody) == LIQUIDITY_TRANSFER,
        DispatchError::Other("V13 liquidity custody allocation mismatch")
    );
    ensure!(
        balance(&marker.input.community_onboarding_custody) == COMMUNITY_TRANSFER,
        DispatchError::Other("V13 community custody allocation mismatch")
    );

    let founding = checked_sum(FOUNDERS.iter().map(|plan| plan.target))?;
    let ecosystem = checked_sum(
        [ECOSYSTEM_TRANSFER, SUDO.target]
            .into_iter()
            .chain(ECOSYSTEM_PRESERVED.iter().map(|item| item.amount)),
    )?;
    let airdrop_validator = checked_sum(
        [COMMUNITY_TRANSFER, REWARD_POT.target]
            .into_iter()
            .chain(AIRDROP_PRESERVED.iter().map(|item| item.amount)),
    )?;
    ensure!(
        founding == FOUNDING_MEMBERS_POOL
            && balance(&marker.input.active_presale_custody) == ACTIVE_PRESALE_POOL
            && ecosystem == ECOSYSTEM_POOL
            && balance(&marker.input.liquidity_custody) == LIQUIDITY_RESERVE_POOL
            && airdrop_validator == AIRDROP_VALIDATOR_POOL,
        DispatchError::Other("V13 retained category allocation mismatch")
    );
    ensure!(
        checked_sum([
            founding,
            ACTIVE_PRESALE_POOL,
            ecosystem,
            LIQUIDITY_RESERVE_POOL,
            airdrop_validator,
        ])? == TARGET_RETAINED_ISSUANCE,
        DispatchError::Other("V13 retained allocations do not sum to target")
    );
    validate_founding_vesting(marker.completed_at)?;
    validate_community_staking_completed()
}

fn validate_completed(marker: &V13Completion<AccountId, BlockNumber>) -> DispatchResult {
    validate_policy_arithmetic()?;
    validate_versions()?;
    ensure!(
        marker.migration_version == MIGRATION_VERSION
            && marker.input.reconciliation_vector_hash == APPROVED_VECTOR_HASH,
        DispatchError::Other("V13 completion marker mismatch")
    );
    ensure!(
        issuance_cap::v13_migration_input::<Runtime>().is_none(),
        DispatchError::Other("V13 deployment input remains after completion")
    );
    ensure!(
        <Balances as Inspect<AccountId>>::total_issuance() == TARGET_RETAINED_ISSUANCE,
        DispatchError::Other("V13 completed issuance mismatch")
    );
    ensure!(
        RemainingAllowance::<Runtime>::get() == Some(MAXIMUM_POST_CORRECTION_NEW_ISSUANCE),
        DispatchError::Other("V13 completed allowance mismatch")
    );
    validate_final_allocations(marker)?;
    validate_reward_liabilities()
}

fn execute_pending(input: V13MigrationInput<AccountId>) -> Result<Weight, DispatchError> {
    validate_pending(&input)?;

    retire_community_staking()?;
    transfer_exact(
        PRESALE_SOURCE,
        &input.active_presale_custody,
        PRESALE_TRANSFER,
    )?;
    transfer_exact(
        ECOSYSTEM_SOURCE,
        &input.ecosystem_custody,
        ECOSYSTEM_TRANSFER,
    )?;
    transfer_exact(
        LIQUIDITY_SOURCE,
        &input.liquidity_custody,
        LIQUIDITY_TRANSFER,
    )?;
    transfer_exact(
        COMMUNITY_SOURCE,
        &input.community_onboarding_custody,
        COMMUNITY_TRANSFER,
    )?;

    for plan in FOUNDERS {
        burn_exact(plan)?;
    }
    for plan in [
        SUDO,
        REWARD_POT,
        PRESALE_SOURCE,
        ECOSYSTEM_SOURCE,
        LIQUIDITY_SOURCE,
        COMMUNITY_SOURCE,
    ] {
        burn_exact(plan)?;
    }

    let completed_at = System::block_number();
    add_founding_vesting(completed_at)?;

    ensure!(
        <Balances as Inspect<AccountId>>::total_issuance() == TARGET_RETAINED_ISSUANCE,
        DispatchError::Other("V13 reconciliation did not reach exact 100M")
    );

    RemainingAllowance::<Runtime>::put(MAXIMUM_POST_CORRECTION_NEW_ISSUANCE);
    StorageVersion::new(1).put::<IssuanceCap>();
    issuance_cap::clear_v13_migration_input();
    let marker = V13Completion {
        migration_version: MIGRATION_VERSION,
        completed_at,
        input,
    };
    V13MigrationCompleted::<Runtime>::put(&marker);
    validate_completed(&marker)?;
    Ok(declared_weight())
}

pub struct V13Migration;

impl OnRuntimeUpgrade for V13Migration {
    fn on_runtime_upgrade() -> Weight {
        if let Some(marker) = V13MigrationCompleted::<Runtime>::get() {
            validate_completed(&marker).unwrap_or_else(|error| {
                panic!("V13 completed-state verification failed: {error:?}")
            });
            return declared_weight();
        }

        let input = issuance_cap::v13_migration_input::<Runtime>()
            .unwrap_or_else(|| panic!("V13 migration deployment input is absent"));
        with_transaction(|| {
            let result = execute_pending(input);
            if result.is_ok() {
                TransactionOutcome::Commit(result)
            } else {
                TransactionOutcome::Rollback(result)
            }
        })
        .unwrap_or_else(|error| panic!("V13 ordered migration failed and rolled back: {error:?}"))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let marker = V13MigrationCompleted::<Runtime>::get();
        let input = issuance_cap::v13_migration_input::<Runtime>();
        if let Some(ref completed) = marker {
            validate_completed(completed)
                .map_err(|_| TryRuntimeError::Other("invalid completed V13 pre-state"))?;
        } else {
            let pending = input.as_ref().ok_or(TryRuntimeError::Other(
                "V13 deployment input absent in pre-state",
            ))?;
            validate_pending(pending)
                .map_err(|_| TryRuntimeError::Other("invalid pending V13 pre-state"))?;
        }
        let liability = reward_liability_summary()
            .map_err(|_| TryRuntimeError::Other("invalid V13 liability pre-state"))?;
        let state = TryPreState {
            marker,
            input,
            issuance: <Balances as Inspect<AccountId>>::total_issuance(),
            allowance: RemainingAllowance::<Runtime>::get(),
            reward_prefix: storage_prefix_fingerprint(b"RewardReserve")?,
            application_prefixes: application_fingerprints()?,
            preserved_accounts: preserved_account_fingerprint(),
            liability,
        };
        Ok(state.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        let before = TryPreState::decode(&mut &state[..])
            .map_err(|_| TryRuntimeError::Other("invalid encoded V13 pre-state"))?;
        let marker = V13MigrationCompleted::<Runtime>::get()
            .ok_or(TryRuntimeError::Other("V13 completion marker absent"))?;
        validate_completed(&marker)
            .map_err(|_| TryRuntimeError::Other("invalid completed V13 post-state"))?;
        if let Some(old_marker) = before.marker {
            ensure_try(
                marker == old_marker
                    && before.input.is_none()
                    && before.issuance == TARGET_RETAINED_ISSUANCE
                    && before.allowance == Some(MAXIMUM_POST_CORRECTION_NEW_ISSUANCE),
                "V13 idempotent state changed",
            )?;
        } else {
            ensure_try(
                before.issuance == R6R3_PRE_MIGRATION_ISSUANCE
                    && before.allowance.is_none()
                    && before.input.as_ref() == Some(&marker.input),
                "V13 pending-to-complete transition mismatch",
            )?;
        }
        ensure_try(
            before.reward_prefix == storage_prefix_fingerprint(b"RewardReserve")?,
            "V13 reward storage changed",
        )?;
        ensure_try(
            before.application_prefixes == application_fingerprints()?,
            "V13 application storage changed",
        )?;
        ensure_try(
            before.preserved_accounts == preserved_account_fingerprint(),
            "V13 preserved account restrictions changed",
        )?;
        ensure_try(
            before.liability
                == reward_liability_summary()
                    .map_err(|_| TryRuntimeError::Other("invalid V13 liability post-state"))?,
            "V13 reward liabilities changed",
        )?;
        ensure_try(
            crate::VERSION.spec_version == 13,
            "runtime spec version changed",
        )?;
        Ok(())
    }
}

#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
struct TryPreState {
    marker: Option<V13Completion<AccountId, BlockNumber>>,
    input: Option<V13MigrationInput<AccountId>>,
    issuance: Balance,
    allowance: Option<Balance>,
    reward_prefix: (u32, [u8; 32]),
    application_prefixes: [(u32, [u8; 32]); 5],
    preserved_accounts: [u8; 32],
    liability: (u32, Balance, [u8; 32]),
}

#[cfg(feature = "try-runtime")]
fn ensure_try(condition: bool, message: &'static str) -> Result<(), TryRuntimeError> {
    if condition {
        Ok(())
    } else {
        Err(TryRuntimeError::Other(message))
    }
}

#[cfg(feature = "try-runtime")]
fn storage_prefix_fingerprint(name: &[u8]) -> Result<(u32, [u8; 32]), TryRuntimeError> {
    const MAX_KEYS: u32 = 4_096;
    let prefix = sp_io::hashing::twox_128(name);
    let mut cursor = prefix.to_vec();
    let mut count = 0u32;
    let mut digest = [0u8; 32];
    loop {
        let Some(key) = sp_io::storage::next_key(&cursor) else {
            break;
        };
        if !key.starts_with(&prefix) {
            break;
        }
        count = count
            .checked_add(1)
            .ok_or(TryRuntimeError::Other("V13 prefix key count overflow"))?;
        ensure_try(count <= MAX_KEYS, "V13 prefix fingerprint exceeds bound")?;
        let value = sp_io::storage::get(&key)
            .ok_or(TryRuntimeError::Other("V13 prefix key disappeared"))?;
        digest = sp_io::hashing::blake2_256(&(digest, key.as_slice(), value.as_ref()).encode());
        cursor = key;
    }
    Ok((count, digest))
}

#[cfg(feature = "try-runtime")]
fn application_fingerprints() -> Result<[(u32, [u8; 32]); 5], TryRuntimeError> {
    Ok([
        storage_prefix_fingerprint(b"Multisig")?,
        storage_prefix_fingerprint(b"Proxy")?,
        storage_prefix_fingerprint(b"Assets")?,
        storage_prefix_fingerprint(b"Nfts")?,
        storage_prefix_fingerprint(b"EraWorlds")?,
    ])
}

#[cfg(feature = "try-runtime")]
fn preserved_account_fingerprint() -> [u8; 32] {
    let mut digest = [0u8; 32];
    for item in AIRDROP_PRESERVED.iter().chain(ECOSYSTEM_PRESERVED.iter()) {
        let who = account(item.account);
        let keys = [
            frame_system::Account::<Runtime>::hashed_key_for(&who),
            pallet_balances::Locks::<Runtime>::hashed_key_for(&who),
            pallet_balances::Reserves::<Runtime>::hashed_key_for(&who),
            pallet_balances::Holds::<Runtime>::hashed_key_for(&who),
            pallet_balances::Freezes::<Runtime>::hashed_key_for(&who),
        ];
        for key in keys {
            let value = sp_io::storage::get(&key);
            digest = sp_io::hashing::blake2_256(&(digest, key, value).encode());
        }
    }
    digest
}
