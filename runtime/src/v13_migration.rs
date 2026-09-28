//! Ordered, transactional V13 retained-supply reconciliation.
//!
//! This is a concrete one-shot runtime migration. The three founding-custody destinations are
//! deterministic keyless pallet subaccounts; only the unrelated community-onboarding destination
//! remains in the deployment input. All source accounts, amounts and invariants are compiled.

use crate::v14_migration_lifecycle::{raw_value, required_version, version};
use alloc::vec::Vec;
use codec::{Decode, DecodeAll, Encode};
use era_v14_custody_governance::CustodyCategory;

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
    AccountId, Assets, Balance, Balances, BlockNumber, EraWorlds, FounderCustody,
    FounderCustodySigners, IssuanceCap, Multisig, Nfts, Proxy, RewardReserve, Runtime,
    RuntimeHoldReason, Staking, System, Vesting,
};

pub const MIGRATION_VERSION: u16 = 13;
pub const R6R3_PRE_MIGRATION_ISSUANCE: Balance = 1_005_003_999_999_999_994_375_836_000;
pub const REQUIRED_DESTRUCTION: Balance = 905_003_999_999_999_994_375_836_000;
pub const MAX_REWARD_LIABILITY_ENTRIES: u32 = 512;
const CURRENT_COMPLETION_LENGTH: usize = 70;
const LEGACY_COMPLETION_LENGTH: usize = 166;

/// Exact V13 completion schema installed before category destinations became deterministic.
/// The three former custody destinations are accepted only from this canonical stored record.
#[derive(Clone, Decode, Encode, Eq, PartialEq)]
struct LegacyV13MigrationInput<AccountId> {
    reconciliation_vector_hash: [u8; 32],
    active_presale_custody: AccountId,
    ecosystem_custody: AccountId,
    liquidity_custody: AccountId,
    community_onboarding_custody: AccountId,
}

#[derive(Clone, Decode, Encode, Eq, PartialEq)]
struct LegacyV13Completion<AccountId, BlockNumber> {
    migration_version: u16,
    completed_at: BlockNumber,
    input: LegacyV13MigrationInput<AccountId>,
}

#[derive(Clone, Decode, Encode, Eq, PartialEq)]
enum CompletedMarker {
    Legacy(LegacyV13Completion<AccountId, BlockNumber>),
    Current(V13Completion<AccountId, BlockNumber>),
}

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

const LEGACY_COMPLETED_AT: BlockNumber = 2_071_915;
const LEGACY_PRESALE_DESTINATION: [u8; 32] =
    decode_hex(b"b6bdd198bb94fc01298ece5c3f28a9321538648228435120df6280bbd2f6b036");
const LEGACY_ECOSYSTEM_DESTINATION: [u8; 32] =
    decode_hex(b"8ea320779dd7d15065698a50b62e8b43a2a90fe82cee462913c9b4b26731b81d");
const LEGACY_LIQUIDITY_DESTINATION: [u8; 32] =
    decode_hex(b"1ec05d5366ba036490b5dd53314f56f3dc370aa34e9dccb983fdb77b027c5127");
const LEGACY_COMMUNITY_DESTINATION: [u8; 32] =
    decode_hex(b"caff7a8ef895d0c81bd57a446f067aca4fda9366a712638a0345ddaa20347f14");
const LEGACY_PRESALE_ACCOUNT_INFO_SHA256: [u8; 32] =
    decode_hex(b"9f6ff3762651d8c9d88dfa205980607fff1856c72f00c13e5728b35bdc381155");
const LEGACY_ECOSYSTEM_ACCOUNT_INFO_SHA256: [u8; 32] =
    decode_hex(b"8c02b77c4a4bc4a9247f108acbb60cdca49badadd75abb6c94e8905c46798406");
const LEGACY_LIQUIDITY_ACCOUNT_INFO_SHA256: [u8; 32] =
    decode_hex(b"14ded94a6bd157de80bc52fc3990f89b6c13648fa34a62ad6129a437af3c053a");
const OPERATIONAL_SUDO_ACCOUNT_INFO_SHA256: [u8; 32] =
    decode_hex(b"bc9eaa729fea97c62f537be72a668a3ee286567e73369ea404df371d62408f57");
const OPERATIONAL_SUDO_POST_UPGRADE_ACCOUNT_INFO_SHA256: [u8; 32] =
    decode_hex(b"71b8ce940a06e5dbd3a25fd360dfe40266df3d07895cfd3e378e0e4f81765e64");
const OPERATIONAL_SUDO_SEALED_NONCE: u32 = 22;
const OPERATIONAL_FEE_ACCOUNT_INFO_SHA256: [u8; 32] =
    decode_hex(b"22e960367908f6cadc2b7dda7a0c3f1fde0bd4c2bba43e525d5b514d3d8d857c");
const OPERATIONAL_TREASURY_ACCOUNT_INFO_SHA256: [u8; 32] = OPERATIONAL_FEE_ACCOUNT_INFO_SHA256;

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
const ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL: Balance = ECOSYSTEM_TRANSFER;
const ECOSYSTEM_OPERATIONAL_RETAINED: Balance =
    SUDO_TARGET_FREE + ECOSYSTEM_PRESERVED[0].amount + ECOSYSTEM_PRESERVED[1].amount;
const ECOSYSTEM_ALLOCATION_TOTAL: Balance = ECOSYSTEM_POOL;
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

const _: () = assert!(
    ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL + ECOSYSTEM_OPERATIONAL_RETAINED
        == ECOSYSTEM_ALLOCATION_TOTAL
);

#[cfg(test)]
pub(crate) fn test_input() -> V13MigrationInput<AccountId> {
    V13MigrationInput {
        reconciliation_vector_hash: APPROVED_VECTOR_HASH,
        community_onboarding_destination: AccountId::new([0xf3; 32]),
    }
}

pub(crate) fn custody_destinations() -> [AccountId; 3] {
    [
        FounderCustody::custody_account(CustodyCategory::Presale),
        FounderCustody::custody_account(CustodyCategory::Ecosystem),
        FounderCustody::custody_account(CustodyCategory::Liquidity),
    ]
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
pub(crate) fn test_custody_sources() -> [AccountId; 3] {
    [
        account(PRESALE_SOURCE.account),
        account(ECOSYSTEM_SOURCE.account),
        account(LIQUIDITY_SOURCE.account),
    ]
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
pub(crate) fn test_legacy_destinations() -> [AccountId; 3] {
    [
        account(LEGACY_PRESALE_DESTINATION),
        account(LEGACY_ECOSYSTEM_DESTINATION),
        account(LEGACY_LIQUIDITY_DESTINATION),
    ]
}

#[cfg(test)]
pub(crate) fn test_legacy_community_destination() -> AccountId {
    account(LEGACY_COMMUNITY_DESTINATION)
}

#[cfg(test)]
pub(crate) fn test_legacy_completed_at() -> BlockNumber {
    LEGACY_COMPLETED_AT
}

#[cfg(test)]
pub(crate) fn test_legacy_marker_bytes() -> Vec<u8> {
    let marker = LegacyV13Completion {
        migration_version: MIGRATION_VERSION,
        completed_at: LEGACY_COMPLETED_AT,
        input: LegacyV13MigrationInput {
            reconciliation_vector_hash: APPROVED_VECTOR_HASH,
            active_presale_custody: account(LEGACY_PRESALE_DESTINATION),
            ecosystem_custody: account(LEGACY_ECOSYSTEM_DESTINATION),
            liquidity_custody: account(LEGACY_LIQUIDITY_DESTINATION),
            community_onboarding_custody: account(LEGACY_COMMUNITY_DESTINATION),
        },
    };
    let bytes = marker.encode();
    assert_eq!(bytes.len(), LEGACY_COMPLETION_LENGTH);
    assert_eq!(
        sp_io::hashing::sha2_256(&bytes),
        decode_hex(b"aaebff35a3fab18c3f10deec7a11d3bbbe2f41a35054f6333d3a04dd75b50972")
    );
    bytes
}

#[cfg(test)]
pub(crate) fn test_transfer_amounts() -> [Balance; 3] {
    [
        PRESALE_TRANSFER,
        ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL,
        LIQUIDITY_TRANSFER,
    ]
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

fn ecosystem_accounting_is_exact() -> bool {
    ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL
        .checked_add(SUDO_TARGET_FREE)
        .and_then(|value| value.checked_add(ECOSYSTEM_PRESERVED[0].amount))
        .and_then(|value| value.checked_add(ECOSYSTEM_PRESERVED[1].amount))
        == Some(ECOSYSTEM_ALLOCATION_TOTAL)
        && ECOSYSTEM_OPERATIONAL_RETAINED == 1_000_300_000_000_000_000
}

fn balance_auxiliary_storage_absent(who: &AccountId) -> bool {
    !pallet_balances::Locks::<Runtime>::contains_key(who)
        && !pallet_balances::Reserves::<Runtime>::contains_key(who)
        && !pallet_balances::Holds::<Runtime>::contains_key(who)
        && !pallet_balances::Freezes::<Runtime>::contains_key(who)
}

const ACCOUNT_INFO_LENGTH: usize = 80;

fn sealed_account_info_bytes(who: &AccountId) -> Result<[u8; ACCOUNT_INFO_LENGTH], DispatchError> {
    let key = frame_system::Account::<Runtime>::hashed_key_for(who);
    let length = sp_io::storage::read(&key, &mut [], 0)
        .ok_or(DispatchError::Other("V13 sealed AccountInfo is absent"))?;
    ensure!(
        length == ACCOUNT_INFO_LENGTH as u32,
        DispatchError::Other("V13 sealed AccountInfo length mismatch")
    );
    let mut bytes = [0u8; ACCOUNT_INFO_LENGTH];
    let read = sp_io::storage::read(&key, &mut bytes, 0)
        .ok_or(DispatchError::Other("V13 sealed AccountInfo disappeared"))?;
    ensure!(
        read == length,
        DispatchError::Other("V13 sealed AccountInfo read length mismatch")
    );
    Ok(bytes)
}

fn validate_sealed_account_info(who: &AccountId, expected_sha256: [u8; 32]) -> DispatchResult {
    ensure!(
        sp_io::hashing::sha2_256(&sealed_account_info_bytes(who)?) == expected_sha256,
        DispatchError::Other("V13 sealed AccountInfo hash mismatch")
    );
    Ok(())
}

fn validate_operational_sudo_account_info(who: &AccountId) -> DispatchResult {
    let bytes = sealed_account_info_bytes(who)?;
    ensure!(
        sp_io::hashing::sha2_256(&bytes) == OPERATIONAL_SUDO_POST_UPGRADE_ACCOUNT_INFO_SHA256,
        DispatchError::Other("V13 post-upgrade Sudo AccountInfo hash mismatch")
    );

    let info = frame_system::AccountInfo::<u32, pallet_balances::AccountData<Balance>>::decode_all(
        &mut &bytes[..],
    )
    .map_err(|_| DispatchError::Other("V13 malformed Sudo AccountInfo"))?;
    let expected_nonce = OPERATIONAL_SUDO_SEALED_NONCE
        .checked_add(1)
        .ok_or(DispatchError::Other("V13 Sudo nonce overflow"))?;
    ensure!(
        info.nonce == expected_nonce
            && info.consumers == 0
            && info.providers == 1
            && info.sufficients == 0
            && info.data.free == SUDO_TARGET_FREE
            && info.data.reserved == 0
            && info.data.frozen == 0,
        DispatchError::Other("V13 post-upgrade Sudo AccountInfo fields mismatch")
    );

    let mut sealed = info;
    sealed.nonce = OPERATIONAL_SUDO_SEALED_NONCE;
    let sealed_bytes = sealed.encode();
    ensure!(
        sealed_bytes.len() == ACCOUNT_INFO_LENGTH
            && sp_io::hashing::sha2_256(&sealed_bytes) == OPERATIONAL_SUDO_ACCOUNT_INFO_SHA256,
        DispatchError::Other("V13 Sudo AccountInfo does not match sealed pre-upgrade state")
    );
    Ok(())
}

fn validate_plain_account(who: &AccountId, expected: Balance) -> DispatchResult {
    let info = frame_system::Account::<Runtime>::get(who);
    ensure!(
        frame_system::Account::<Runtime>::contains_key(who)
            && info.consumers == 0
            && info.providers == 1
            && info.sufficients == 0
            && info.data.free == expected
            && info.data.reserved == 0
            && info.data.frozen == 0
            && balance(who) == expected
            && <Balances as Inspect<AccountId>>::reducible_balance(
                who,
                Preservation::Expendable,
                Fortitude::Polite,
            ) == expected
            && balance_auxiliary_storage_absent(who),
        DispatchError::Other("V13 legacy bridge account state mismatch")
    );
    Ok(())
}

fn validate_pristine_destination(who: &AccountId) -> DispatchResult {
    let info = frame_system::Account::<Runtime>::get(who);
    ensure!(
        info.nonce == 0
            && info.consumers == 0
            && info.providers == 0
            && info.sufficients == 0
            && info.data.free == 0
            && info.data.reserved == 0
            && info.data.frozen == 0
            && balance(who) == 0
            && balance_auxiliary_storage_absent(who),
        DispatchError::Other("V13 legacy bridge destination is not pristine")
    );
    Ok(())
}

fn operational_accounts() -> Result<[AccountId; 3], DispatchError> {
    let sudo = pallet_sudo::Key::<Runtime>::get()
        .ok_or(DispatchError::Other("V13 operational Sudo account missing"))?;
    let fee = RewardReserve::fee_collection_account();
    let treasury = RewardReserve::ecosystem_treasury_account();
    ensure!(
        sudo == account(SUDO.account)
            && fee == account(ECOSYSTEM_PRESERVED[0].account)
            && treasury == account(ECOSYSTEM_PRESERVED[1].account)
            && sudo != fee
            && sudo != treasury
            && fee != treasury,
        DispatchError::Other("V13 operational account identity mismatch")
    );
    Ok([sudo, fee, treasury])
}

fn validate_operational_accounts() -> DispatchResult {
    ensure!(
        ecosystem_accounting_is_exact(),
        DispatchError::Other("V13 ecosystem allocation arithmetic mismatch")
    );
    let [sudo, fee, treasury] = operational_accounts()?;
    validate_plain_account(&sudo, SUDO_TARGET_FREE)?;
    validate_operational_sudo_account_info(&sudo)?;
    validate_plain_account(&fee, ECOSYSTEM_PRESERVED[0].amount)?;
    validate_sealed_account_info(&fee, OPERATIONAL_FEE_ACCOUNT_INFO_SHA256)?;
    validate_plain_account(&treasury, ECOSYSTEM_PRESERVED[1].amount)?;
    validate_sealed_account_info(&treasury, OPERATIONAL_TREASURY_ACCOUNT_INFO_SHA256)
}

#[cfg(test)]
pub(crate) fn test_validate_operational_sudo_account_info() -> DispatchResult {
    validate_operational_sudo_account_info(&test_sudo_account())
}

#[cfg(test)]
pub(crate) fn test_validate_pre_upgrade_sudo_account_info() -> DispatchResult {
    validate_sealed_account_info(&test_sudo_account(), OPERATIONAL_SUDO_ACCOUNT_INFO_SHA256)
}

fn legacy_destinations(marker: &LegacyV13Completion<AccountId, BlockNumber>) -> [AccountId; 3] {
    [
        marker.input.active_presale_custody.clone(),
        marker.input.ecosystem_custody.clone(),
        marker.input.liquidity_custody.clone(),
    ]
}

fn validate_no_prior_custody_operation() -> DispatchResult {
    for category in [
        CustodyCategory::Presale,
        CustodyCategory::Ecosystem,
        CustodyCategory::Liquidity,
    ] {
        ensure!(
            !era_v14_custody_governance::PendingWithdrawal::<Runtime>::contains_key(category)
                && !era_v14_custody_governance::NextRequestId::<Runtime>::contains_key(category),
            DispatchError::Other("V13 legacy bridge custody state is not pristine")
        );
    }
    Ok(())
}

fn identities_are_distinct(identities: &[AccountId]) -> bool {
    identities.iter().enumerate().all(|(index, identity)| {
        identity != &AccountId::new([0; 32])
            && identities[..index].iter().all(|prior| prior != identity)
    })
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
    if required_version::<P>() != Ok(expected) {
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
    validate_versions_for_initialization(false)
}

/// Validate the version keys needed by the completed-V13 bridge without walking any pallet
/// prefix. The bridge is deliberately confined to a fixed, reviewable set of storage keys.
fn validate_bridge_versions() -> DispatchResult {
    ensure!(
        version::<IssuanceCap>()? == Some(StorageVersion::new(1)),
        DispatchError::Other("V13 IssuanceCap storage version is not supported")
    );
    let reward_version = required_version::<RewardReserve>()?;
    ensure!(
        reward_version == StorageVersion::new(2) || reward_version == StorageVersion::new(3),
        DispatchError::Other(
            "V13 RewardReserve storage version is not 2 or append-only V14 version 3"
        )
    );
    ensure!(
        required_version::<Multisig>()? == StorageVersion::new(1),
        DispatchError::Other("V13 Multisig storage version mismatch")
    );
    ensure!(
        required_version::<Proxy>()? == StorageVersion::new(0),
        DispatchError::Other("V13 Proxy storage version mismatch")
    );
    ensure!(
        required_version::<Assets>()? == StorageVersion::new(1),
        DispatchError::Other("V13 Assets storage version mismatch")
    );
    ensure!(
        required_version::<Nfts>()? == StorageVersion::new(1),
        DispatchError::Other("V13 Nfts storage version mismatch")
    );
    ensure!(
        required_version::<EraWorlds>()? == StorageVersion::new(1),
        DispatchError::Other("V13 EraWorlds storage version mismatch")
    );
    Ok(())
}

fn validate_versions_for_initialization(initial: bool) -> DispatchResult {
    match version::<IssuanceCap>()? {
        None if initial => {
            crate::v14_migration_lifecycle::pristine_namespace::<IssuanceCap>(false)?
        }
        Some(v) if v == StorageVersion::new(1) => {}
        _ => {
            return Err(DispatchError::Other(
                "V13 IssuanceCap storage version is not supported",
            ))
        }
    }
    let reward_version = required_version::<RewardReserve>()?;
    ensure!(
        reward_version == StorageVersion::new(2) || reward_version == StorageVersion::new(3),
        DispatchError::Other(
            "V13 RewardReserve storage version is not 2 or append-only V14 version 3"
        )
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

fn validate_custody_identities() -> DispatchResult {
    let destinations = custody_destinations();
    let signers = FounderCustodySigners::get();
    ensure!(
        signers[0] < signers[1] && signers[1] < signers[2],
        DispatchError::Other("V13 custody signers are not canonical and distinct")
    );

    let identities = [
        account(PRESALE_SOURCE.account),
        account(ECOSYSTEM_SOURCE.account),
        account(LIQUIDITY_SOURCE.account),
        destinations[0].clone(),
        destinations[1].clone(),
        destinations[2].clone(),
        signers[0].clone(),
        signers[1].clone(),
        signers[2].clone(),
    ];
    for (index, identity) in identities.iter().enumerate() {
        ensure!(
            identity != &AccountId::new([0; 32])
                && identities[..index]
                    .iter()
                    .all(|earlier| earlier != identity),
            DispatchError::Other("V13 legacy, custody, and signer identities are not distinct")
        );
    }

    let standard_controller = account(decode_hex(
        b"a3867a592cdfe4142f19f0f709842c9f3e48af3ecbfd77308bdf533c3ee86158",
    ));
    ensure!(
        !destinations.contains(&standard_controller),
        DispatchError::Other("V13 standard multisig controller is a custody destination")
    );
    Ok(())
}

fn validate_input(input: &V13MigrationInput<AccountId>) -> DispatchResult {
    ensure!(
        input.reconciliation_vector_hash == APPROVED_VECTOR_HASH,
        DispatchError::Other("V13 reconciliation vector hash mismatch")
    );
    validate_custody_identities()?;
    let custody = custody_destinations();
    let destinations = [
        custody[0].clone(),
        custody[1].clone(),
        custody[2].clone(),
        input.community_onboarding_destination.clone(),
    ];
    let known = all_known_accounts();
    for (index, destination) in destinations.iter().enumerate() {
        ensure!(
            balance(destination) == 0,
            DispatchError::Other("V13 destination is not empty")
        );
        ensure!(
            !frame_system::Account::<Runtime>::contains_key(destination),
            DispatchError::Other("V13 destination already has system state")
        );
        ensure!(
            !known
                .iter()
                .any(|known_account| known_account == destination),
            DispatchError::Other("V13 destination aliases a vector account")
        );
        ensure!(
            destinations[..index]
                .iter()
                .all(|earlier| earlier != destination),
            DispatchError::Other("V13 destinations are not distinct")
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
    validate_versions_for_initialization(true)?;
    ensure!(
        <Balances as Inspect<AccountId>>::total_issuance() == R6R3_PRE_MIGRATION_ISSUANCE,
        DispatchError::Other("V13 pre-migration issuance mismatch")
    );
    ensure!(
        !sp_io::storage::exists(&RemainingAllowance::<Runtime>::hashed_key()),
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
    let custody = custody_destinations();
    ensure!(
        balance(&custody[0]) == PRESALE_TRANSFER,
        DispatchError::Other("V13 presale custody allocation mismatch")
    );
    ensure!(
        balance(&custody[1]) == ECOSYSTEM_TRANSFER,
        DispatchError::Other("V13 ecosystem custody allocation mismatch")
    );
    ensure!(
        balance(&custody[2]) == LIQUIDITY_TRANSFER,
        DispatchError::Other("V13 liquidity custody allocation mismatch")
    );
    ensure!(
        balance(&marker.input.community_onboarding_destination) == COMMUNITY_TRANSFER,
        DispatchError::Other("V13 community destination allocation mismatch")
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
            && balance(&custody[0]) == ACTIVE_PRESALE_POOL
            && ecosystem == ECOSYSTEM_POOL
            && balance(&custody[2]) == LIQUIDITY_RESERVE_POOL
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
    validate_completed_state(marker)
}

fn validate_bridge_completed(marker: &V13Completion<AccountId, BlockNumber>) -> DispatchResult {
    validate_policy_arithmetic()?;
    validate_bridge_versions()?;
    validate_completed_state(marker)
}

fn validate_completed_state(marker: &V13Completion<AccountId, BlockNumber>) -> DispatchResult {
    ensure!(
        marker.migration_version == MIGRATION_VERSION
            && marker.input.reconciliation_vector_hash == APPROVED_VECTOR_HASH,
        DispatchError::Other("V13 completion marker mismatch")
    );
    ensure!(
        !sp_io::storage::exists(&issuance_cap::v13_migration_input_key()),
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

/// The protected completion marker, not the current spec version, is authoritative.
/// Keep immutable identity/version/cap/liability checks, but do not freeze later balances,
/// claims, application state or completed vesting schedules at the reconciliation snapshot.
fn decode_canonical<T: DecodeAll + Encode>(bytes: &[u8]) -> Result<T, DispatchError> {
    let value = T::decode_all(&mut &bytes[..])
        .map_err(|_| DispatchError::Other("V13 completion marker decoding failed"))?;
    ensure!(
        value.encode() == bytes,
        DispatchError::Other("V13 completion marker is not canonical SCALE")
    );
    Ok(value)
}

fn completed_marker() -> Result<Option<CompletedMarker>, DispatchError> {
    let key = V13MigrationCompleted::<Runtime>::hashed_key();
    let Some(length) = sp_io::storage::read(&key, &mut [], 0) else {
        return Ok(None);
    };
    ensure!(
        length <= LEGACY_COMPLETION_LENGTH as u32,
        DispatchError::Other("V13 completion marker exceeds bounded schema")
    );

    let mut bytes = [0u8; LEGACY_COMPLETION_LENGTH];
    let read = sp_io::storage::read(&key, &mut bytes, 0)
        .ok_or(DispatchError::Other("V13 completion marker disappeared"))?;
    ensure!(
        read == length,
        DispatchError::Other("V13 completion marker changed during read")
    );

    match length as usize {
        CURRENT_COMPLETION_LENGTH => Ok(Some(CompletedMarker::Current(decode_canonical(
            &bytes[..CURRENT_COMPLETION_LENGTH],
        )?))),
        LEGACY_COMPLETION_LENGTH => Ok(Some(CompletedMarker::Legacy(decode_canonical(
            &bytes[..LEGACY_COMPLETION_LENGTH],
        )?))),
        _ => Err(DispatchError::Other(
            "V13 completion marker has unknown encoded length",
        )),
    }
}

fn validate_legacy_marker(marker: &LegacyV13Completion<AccountId, BlockNumber>) -> DispatchResult {
    ensure!(
        marker.migration_version == MIGRATION_VERSION
            && marker.completed_at == LEGACY_COMPLETED_AT
            && marker.completed_at <= System::block_number()
            && marker.input.reconciliation_vector_hash == APPROVED_VECTOR_HASH
            && marker.input.active_presale_custody == account(LEGACY_PRESALE_DESTINATION)
            && marker.input.ecosystem_custody == account(LEGACY_ECOSYSTEM_DESTINATION)
            && marker.input.liquidity_custody == account(LEGACY_LIQUIDITY_DESTINATION)
            && marker.input.community_onboarding_custody == account(LEGACY_COMMUNITY_DESTINATION),
        DispatchError::Other("V13 legacy completion marker does not match sealed evidence")
    );
    Ok(())
}

fn validate_legacy_completed(
    marker: &LegacyV13Completion<AccountId, BlockNumber>,
) -> DispatchResult {
    validate_policy_arithmetic()?;
    validate_bridge_versions()?;
    validate_legacy_marker(marker)?;
    ensure!(
        completed_marker()? == Some(CompletedMarker::Legacy(marker.clone())),
        DispatchError::Other("V13 legacy completion marker changed during validation")
    );
    ensure!(
        !sp_io::storage::exists(&issuance_cap::v13_migration_input_key()),
        DispatchError::Other("V13 deployment input remains with legacy completion")
    );
    ensure!(
        <Balances as Inspect<AccountId>>::total_issuance() == TARGET_RETAINED_ISSUANCE,
        DispatchError::Other("V13 legacy completed issuance mismatch")
    );
    ensure!(
        RemainingAllowance::<Runtime>::get() == Some(MAXIMUM_POST_CORRECTION_NEW_ISSUANCE),
        DispatchError::Other("V13 legacy completed allowance mismatch")
    );

    validate_custody_identities()?;
    validate_no_prior_custody_operation()?;
    validate_operational_accounts()?;

    let legacy = legacy_destinations(marker);
    let current = custody_destinations();
    let signers = FounderCustodySigners::get();
    let community = marker.input.community_onboarding_custody.clone();
    let operational = operational_accounts()?;
    let identities = [
        legacy[0].clone(),
        legacy[1].clone(),
        legacy[2].clone(),
        current[0].clone(),
        current[1].clone(),
        current[2].clone(),
        signers[0].clone(),
        signers[1].clone(),
        signers[2].clone(),
        community.clone(),
        operational[0].clone(),
        operational[1].clone(),
        operational[2].clone(),
    ];
    ensure!(
        identities_are_distinct(&identities),
        DispatchError::Other("V13 legacy bridge identities are not mutually distinct")
    );
    let known = all_known_accounts();
    for destination in legacy.iter().chain(current.iter()).chain([&community]) {
        ensure!(
            !known.contains(destination),
            DispatchError::Other("V13 legacy bridge destination aliases a vector account")
        );
    }

    validate_plain_account(&legacy[0], PRESALE_TRANSFER)?;
    validate_sealed_account_info(&legacy[0], LEGACY_PRESALE_ACCOUNT_INFO_SHA256)?;
    validate_plain_account(&legacy[1], ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL)?;
    validate_sealed_account_info(&legacy[1], LEGACY_ECOSYSTEM_ACCOUNT_INFO_SHA256)?;
    validate_plain_account(&legacy[2], LIQUIDITY_TRANSFER)?;
    validate_sealed_account_info(&legacy[2], LEGACY_LIQUIDITY_ACCOUNT_INFO_SHA256)?;
    validate_plain_account(&community, COMMUNITY_TRANSFER)?;
    for destination in &current {
        validate_pristine_destination(destination)?;
    }

    for plan in FOUNDERS {
        ensure_burn_plan_after(plan)?;
    }
    for plan in [
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
    validate_founding_vesting(marker.completed_at)?;
    validate_community_staking_completed()?;
    validate_reward_liabilities()
}

fn transfer_account_exact(
    source: &AccountId,
    destination: &AccountId,
    amount: Balance,
) -> DispatchResult {
    let transferred = <Balances as Mutate<AccountId>>::transfer(
        source,
        destination,
        amount,
        Preservation::Expendable,
    )?;
    ensure!(
        transferred == amount,
        DispatchError::Other("V13 legacy bridge transfer was not exact")
    );
    Ok(())
}

fn current_marker_from_legacy(
    marker: &LegacyV13Completion<AccountId, BlockNumber>,
) -> V13Completion<AccountId, BlockNumber> {
    V13Completion {
        migration_version: marker.migration_version,
        completed_at: marker.completed_at,
        input: V13MigrationInput {
            reconciliation_vector_hash: marker.input.reconciliation_vector_hash,
            community_onboarding_destination: marker.input.community_onboarding_custody.clone(),
        },
    }
}

fn validate_legacy_bridge_poststate(
    legacy_marker: &LegacyV13Completion<AccountId, BlockNumber>,
    current_marker: &V13Completion<AccountId, BlockNumber>,
    issuance_before: Balance,
    operational_before: &[frame_system::AccountInfo<u32, pallet_balances::AccountData<Balance>>; 3],
) -> DispatchResult {
    ensure!(
        completed_marker()? == Some(CompletedMarker::Current(current_marker.clone())),
        DispatchError::Other("V13 legacy bridge current marker mismatch")
    );
    validate_bridge_completed(current_marker)?;
    ensure!(
        <Balances as Inspect<AccountId>>::total_issuance() == issuance_before,
        DispatchError::Other("V13 legacy bridge changed total issuance")
    );

    let legacy = legacy_destinations(legacy_marker);
    for destination in &legacy {
        validate_pristine_destination(destination)?;
    }
    let current = custody_destinations();
    validate_plain_account(&current[0], PRESALE_TRANSFER)?;
    validate_plain_account(&current[1], ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL)?;
    validate_plain_account(&current[2], LIQUIDITY_TRANSFER)?;
    validate_no_prior_custody_operation()?;
    validate_operational_accounts()?;
    let operational = operational_accounts()?;
    for (index, who) in operational.iter().enumerate() {
        ensure!(
            frame_system::Account::<Runtime>::get(who) == operational_before[index],
            DispatchError::Other("V13 legacy bridge changed an operational account")
        );
    }
    Ok(())
}

fn execute_legacy_bridge_with_checkpoint<F>(
    marker: LegacyV13Completion<AccountId, BlockNumber>,
    mut checkpoint: F,
) -> Result<Weight, DispatchError>
where
    F: FnMut(u8) -> DispatchResult,
{
    // This is deliberately the only pre-write section. Every marker, identity, balance,
    // reference, allocation and custody invariant is validated before the first transfer.
    validate_legacy_completed(&marker)?;
    let issuance_before = <Balances as Inspect<AccountId>>::total_issuance();
    let operational = operational_accounts()?;
    let operational_before = operational
        .each_ref()
        .map(frame_system::Account::<Runtime>::get);
    let legacy = legacy_destinations(&marker);
    let current = custody_destinations();

    transfer_account_exact(&legacy[0], &current[0], PRESALE_TRANSFER)?;
    checkpoint(1)?;
    transfer_account_exact(&legacy[1], &current[1], ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL)?;
    checkpoint(2)?;
    transfer_account_exact(&legacy[2], &current[2], LIQUIDITY_TRANSFER)?;
    checkpoint(3)?;

    let current_marker = current_marker_from_legacy(&marker);
    V13MigrationCompleted::<Runtime>::put(&current_marker);
    checkpoint(4)?;
    validate_legacy_bridge_poststate(
        &marker,
        &current_marker,
        issuance_before,
        &operational_before,
    )?;
    Ok(declared_weight())
}

fn execute_legacy_bridge(
    marker: LegacyV13Completion<AccountId, BlockNumber>,
) -> Result<Weight, DispatchError> {
    execute_legacy_bridge_with_checkpoint(marker, |_| Ok(()))
}

#[cfg(test)]
pub(crate) fn test_execute_legacy_bridge(fail_at: Option<u8>) -> Result<Weight, DispatchError> {
    let marker = match completed_marker()? {
        Some(CompletedMarker::Legacy(marker)) => marker,
        _ => return Err(DispatchError::Other("test legacy marker absent")),
    };
    with_transaction(|| {
        let result = execute_legacy_bridge_with_checkpoint(marker, |checkpoint| {
            ensure!(
                fail_at != Some(checkpoint),
                DispatchError::Other("injected V13 legacy bridge failure")
            );
            Ok(())
        });
        if result.is_ok() {
            TransactionOutcome::Commit(result)
        } else {
            TransactionOutcome::Rollback(result)
        }
    })
}

#[cfg(test)]
pub(crate) fn test_ecosystem_accounting_is_exact() -> bool {
    ecosystem_accounting_is_exact()
}
fn validate_replay(marker: &V13Completion<AccountId, BlockNumber>) -> DispatchResult {
    validate_policy_arithmetic()?;
    ensure!(
        required_version::<IssuanceCap>()? == StorageVersion::new(1),
        DispatchError::Other("V13 replay issuance-cap version mismatch")
    );
    let reward_version = required_version::<RewardReserve>()?;
    ensure!(
        reward_version == StorageVersion::new(2) || reward_version == StorageVersion::new(3),
        DispatchError::Other("V13 replay reward version mismatch")
    );
    ensure!(
        marker.migration_version == MIGRATION_VERSION
            && marker.input.reconciliation_vector_hash == APPROVED_VECTOR_HASH
            && marker.completed_at > 0
            && marker.completed_at <= System::block_number(),
        DispatchError::Other("V13 replay completion evidence mismatch")
    );
    ensure!(
        !sp_io::storage::exists(&issuance_cap::v13_migration_input_key()),
        DispatchError::Other("V13 deployment input remains after completion")
    );
    validate_custody_identities()?;
    let custody = custody_destinations();
    let destinations = [
        custody[0].clone(),
        custody[1].clone(),
        custody[2].clone(),
        marker.input.community_onboarding_destination.clone(),
    ];
    let known = all_known_accounts();
    for (index, destination) in destinations.iter().enumerate() {
        ensure!(
            !known.contains(destination)
                && destination != &AccountId::new([0; 32])
                && destinations[..index]
                    .iter()
                    .all(|earlier| earlier != destination),
            DispatchError::Other("V13 replay destination identity mismatch")
        );
    }
    let remaining = raw_value::<Balance, 16>(&RemainingAllowance::<Runtime>::hashed_key())?
        .ok_or(DispatchError::Other("V13 replay allowance missing"))?;
    let issuance = <Balances as Inspect<AccountId>>::total_issuance();
    ensure!(
        remaining <= MAXIMUM_POST_CORRECTION_NEW_ISSUANCE
            && issuance
                .checked_add(remaining)
                .is_some_and(|sum| sum <= crate::upgrade13_policy::ABSOLUTE_LIFETIME_CAP),
        DispatchError::Other("V13 replay lifetime cap mismatch")
    );
    ensure!(
        matches!(required_version::<Nfts>()?, v if v == StorageVersion::new(1) || v == StorageVersion::new(2)),
        DispatchError::Other("V13 replay NFT version mismatch")
    );
    for actual in [
        required_version::<Multisig>()?,
        required_version::<Assets>()?,
        required_version::<EraWorlds>()?,
    ] {
        ensure!(
            actual == StorageVersion::new(1),
            DispatchError::Other("V13 replay application version mismatch")
        );
    }
    ensure!(
        required_version::<Proxy>()? == StorageVersion::new(0),
        DispatchError::Other("V13 replay proxy version mismatch")
    );
    let v14 = raw_value::<BlockNumber, 4>(
        &pallet_security_budget::MigrationCompletedAt::<Runtime>::hashed_key(),
    )?;
    if let Some(at) = v14 {
        ensure!(
            version::<crate::SecurityBudget>()? == Some(StorageVersion::new(1))
                && reward_version == StorageVersion::new(3)
                && at >= marker.completed_at
                && at <= System::block_number()
                && crate::SecurityBudget::legacy_reward_liability()
                    == crate::SecurityBudgetExistingEarnedRewardLiability::get()
                && crate::SecurityBudget::accounted_reward_total()?
                    <= crate::SecurityBudgetTotalStakingRewards::get(),
            DispatchError::Other("V13 replay V14 completion evidence mismatch")
        );
        // Telescoping the adopted 90/10 split: gross = 10 * treasury_total - carry.
        // Fee/tip funding is deliberately excluded. Burns cannot change these issuance records.
        let treasury = raw_value::<Balance, 16>(&pallet_security_budget::TreasuryIssuanceTotal::<
            Runtime,
        >::hashed_key())?
        .unwrap_or(0);
        let carry = raw_value::<u8, 1>(
            &pallet_security_budget::IssuanceSplitCarry::<Runtime>::hashed_key(),
        )?
        .unwrap_or(0);
        ensure!(
            carry < 10,
            DispatchError::Other("V13 replay split carry corrupt")
        );
        let gross = treasury
            .checked_mul(10)
            .and_then(|n| n.checked_sub(carry.into()))
            .ok_or(DispatchError::Other(
                "V13 replay issuance evidence overflow",
            ))?;
        ensure!(
            MAXIMUM_POST_CORRECTION_NEW_ISSUANCE.checked_sub(gross) == Some(remaining),
            DispatchError::Other("V13 replay allowance differs from consumed issuance")
        );
    } else {
        // No production issuance route existed before V14's authorized activation.
        ensure!(
            remaining == MAXIMUM_POST_CORRECTION_NEW_ISSUANCE,
            DispatchError::Other("V13 replay unaccounted pre-V14 issuance")
        );
    }
    validate_reward_liabilities()?;
    ensure!(
        RewardReserve::pot_balance()
            >= RewardReserve::committed_liabilities()
                .checked_add(RewardReserve::reward_pot_floor())
                .ok_or(DispatchError::Other("V13 replay liability overflow"))?,
        DispatchError::Other("V13 replay reward insolvency")
    );
    Ok(())
}

fn execute_pending(input: V13MigrationInput<AccountId>) -> Result<Weight, DispatchError> {
    validate_pending(&input)?;
    // Replace the generated initializer only for a fully validated, pristine predecessor.
    // This version write belongs to the same transaction as all V13 reconciliation writes.
    StorageVersion::new(1).put::<IssuanceCap>();

    retire_community_staking()?;
    let custody = custody_destinations();
    transfer_exact(PRESALE_SOURCE, &custody[0], PRESALE_TRANSFER)?;
    transfer_exact(ECOSYSTEM_SOURCE, &custody[1], ECOSYSTEM_TRANSFER)?;
    transfer_exact(LIQUIDITY_SOURCE, &custody[2], LIQUIDITY_TRANSFER)?;
    transfer_exact(
        COMMUNITY_SOURCE,
        &input.community_onboarding_destination,
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
        if crate::fresh_genesis::Enabled::<crate::Runtime>::get() {
            crate::fresh_genesis::validate_lifecycle();
            return <crate::Runtime as frame_system::Config>::DbWeight::get().reads(12);
        }
        match completed_marker() {
            Ok(Some(CompletedMarker::Current(marker))) => {
                // A completed replay is observational. Canonically shaped but semantically
                // invalid evidence aborts; malformed reads returned by the classifier below do
                // not panic or create a write.
                validate_replay(&marker).unwrap_or_else(|error| {
                    panic!("V13 completed-state verification failed: {error:?}")
                });
                return declared_weight();
            }
            Ok(Some(CompletedMarker::Legacy(marker))) => {
                // The bridge is a single storage transaction. Any failed precondition,
                // transfer, checkpoint or postcondition rolls back transfers, events,
                // account references and the completion-marker replacement together.
                return with_transaction(|| {
                    let result = execute_legacy_bridge(marker);
                    if result.is_ok() {
                        TransactionOutcome::Commit(result)
                    } else {
                        TransactionOutcome::Rollback(result)
                    }
                })
                .unwrap_or_else(|error| {
                    panic!("V13 legacy bridge failed and rolled back: {error:?}")
                });
            }
            Err(_) => return declared_weight(),
            Ok(None) => {}
        }

        let input =
            raw_value::<V13MigrationInput<AccountId>, 64>(&issuance_cap::v13_migration_input_key())
                .unwrap_or_else(|error| panic!("V13 migration input decoding failed: {error:?}"))
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
        if crate::fresh_genesis::Enabled::<crate::Runtime>::get() {
            crate::fresh_genesis::validate_lifecycle();
            return Ok(b"ERA_FRESH_V14_GENESIS".to_vec());
        }
        let marker = completed_marker()?;
        let input = raw_value::<V13MigrationInput<AccountId>, 64>(
            &issuance_cap::v13_migration_input_key(),
        )?;
        match marker.as_ref() {
            Some(CompletedMarker::Current(completed)) => validate_replay(completed)
                .map_err(|_| TryRuntimeError::Other("invalid current V13 pre-state"))?,
            Some(CompletedMarker::Legacy(completed)) => validate_legacy_completed(completed)
                .map_err(|_| TryRuntimeError::Other("invalid legacy V13 pre-state"))?,
            None => {
                let pending = input.as_ref().ok_or(TryRuntimeError::Other(
                    "V13 deployment input absent in pre-state",
                ))?;
                validate_pending(pending)
                    .map_err(|_| TryRuntimeError::Other("invalid pending V13 pre-state"))?;
            }
        }
        let liability = reward_liability_summary()
            .map_err(|_| TryRuntimeError::Other("invalid V13 liability pre-state"))?;
        let legacy_operational = if matches!(&marker, Some(CompletedMarker::Legacy(_))) {
            Some(operational_account_fingerprint()?)
        } else {
            None
        };
        let state = TryPreState {
            marker,
            input,
            issuance: <Balances as Inspect<AccountId>>::total_issuance(),
            allowance: RemainingAllowance::<Runtime>::get(),
            reward_prefix: storage_prefix_fingerprint(b"RewardReserve")?,
            application_prefixes: application_fingerprints()?,
            preserved_accounts: preserved_account_fingerprint(),
            legacy_operational,
            liability,
        };
        Ok(state.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        if crate::fresh_genesis::Enabled::<crate::Runtime>::get() {
            if state != b"ERA_FRESH_V14_GENESIS" {
                return Err("fresh-chain upgrade pre-state mismatch".into());
            }
            crate::fresh_genesis::validate_lifecycle();
            return Ok(());
        }
        let before = TryPreState::decode(&mut &state[..])
            .map_err(|_| TryRuntimeError::Other("invalid encoded V13 pre-state"))?;
        let marker =
            completed_marker()?.ok_or(TryRuntimeError::Other("V13 completion marker absent"))?;
        match (before.marker, marker) {
            (Some(CompletedMarker::Current(old)), CompletedMarker::Current(current)) => {
                validate_replay(&current)
                    .map_err(|_| TryRuntimeError::Other("invalid V13 replay post-state"))?;
                ensure_try(
                    current == old
                        && before.input.is_none()
                        && before.issuance == <Balances as Inspect<AccountId>>::total_issuance()
                        && before.allowance == RemainingAllowance::<Runtime>::get()
                        && before.legacy_operational.is_none(),
                    "V13 idempotent state changed",
                )?;
            }
            (Some(CompletedMarker::Legacy(old)), CompletedMarker::Current(current)) => {
                validate_replay(&current)
                    .map_err(|_| TryRuntimeError::Other("invalid V13 bridge post-state"))?;
                let expected = current_marker_from_legacy(&old);
                ensure_try(
                    current == expected
                        && before.input.is_none()
                        && before.issuance == <Balances as Inspect<AccountId>>::total_issuance()
                        && before.allowance == RemainingAllowance::<Runtime>::get(),
                    "V13 legacy-to-current transition mismatch",
                )?;
                for destination in legacy_destinations(&old) {
                    validate_pristine_destination(&destination).map_err(|_| {
                        TryRuntimeError::Other("V13 legacy destination remains funded")
                    })?;
                }
                let current_destinations = custody_destinations();
                for (destination, expected) in current_destinations.iter().zip([
                    PRESALE_TRANSFER,
                    ECOSYSTEM_3_OF_3_CUSTODY_PRINCIPAL,
                    LIQUIDITY_TRANSFER,
                ]) {
                    validate_plain_account(destination, expected).map_err(|_| {
                        TryRuntimeError::Other("V13 current custody allocation mismatch")
                    })?;
                }
                validate_operational_accounts()
                    .map_err(|_| TryRuntimeError::Other("V13 operational allocation changed"))?;
                ensure_try(
                    before.legacy_operational == Some(operational_account_fingerprint()?),
                    "V13 operational account storage changed",
                )?;
            }
            (None, CompletedMarker::Current(current)) => {
                validate_completed(&current)
                    .map_err(|_| TryRuntimeError::Other("invalid initial V13 post-state"))?;
                ensure_try(
                    before.issuance == R6R3_PRE_MIGRATION_ISSUANCE
                        && before.allowance.is_none()
                        && before.input.as_ref() == Some(&current.input)
                        && before.legacy_operational.is_none(),
                    "V13 pending-to-complete transition mismatch",
                )?;
            }
            _ => return Err(TryRuntimeError::Other("invalid V13 marker transition")),
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
            matches!(crate::VERSION.spec_version, 13 | 14 | 15)
                && crate::VERSION.transaction_version == 1,
            "unsupported V13 migration execution runtime",
        )?;
        Ok(())
    }
}

#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
struct TryPreState {
    marker: Option<CompletedMarker>,
    input: Option<V13MigrationInput<AccountId>>,
    issuance: Balance,
    allowance: Option<Balance>,
    reward_prefix: (u32, [u8; 32]),
    application_prefixes: [(u32, [u8; 32]); 5],
    preserved_accounts: [u8; 32],
    legacy_operational: Option<[u8; 32]>,
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

#[cfg(feature = "try-runtime")]
fn operational_account_fingerprint() -> Result<[u8; 32], TryRuntimeError> {
    let accounts = operational_accounts()
        .map_err(|_| TryRuntimeError::Other("V13 operational identities invalid"))?;
    let mut digest = [0u8; 32];
    let sudo_key = pallet_sudo::Key::<Runtime>::hashed_key();
    digest = sp_io::hashing::blake2_256(
        &(digest, sudo_key.as_slice(), sp_io::storage::get(&sudo_key)).encode(),
    );
    for who in accounts {
        for key in [
            frame_system::Account::<Runtime>::hashed_key_for(&who),
            pallet_balances::Locks::<Runtime>::hashed_key_for(&who),
            pallet_balances::Reserves::<Runtime>::hashed_key_for(&who),
            pallet_balances::Holds::<Runtime>::hashed_key_for(&who),
            pallet_balances::Freezes::<Runtime>::hashed_key_for(&who),
        ] {
            let value = sp_io::storage::get(&key);
            digest = sp_io::hashing::blake2_256(&(digest, key, value).encode());
        }
    }
    Ok(digest)
}
