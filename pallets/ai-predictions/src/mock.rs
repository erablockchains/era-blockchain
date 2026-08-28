use crate as pallet_ai_predictions;

use frame_support::{
    construct_runtime, parameter_types,
    traits::{ConstU16, ConstU32, Everything},
    PalletId,
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test
    {
        System: frame_system,
        Balances: pallet_balances,
        AiPredictions: pallet_ai_predictions,
    }
);

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const ExistentialDeposit: u64 = 1;
    pub const AiPredictionsPalletId: PalletId = PalletId(*b"era/aipr");
}

impl frame_system::Config for Test {
    type BaseCallFilter = Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type Nonce = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type BlockHashCount = BlockHashCount;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<u64>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type RuntimeTask = ();
    type ExtensionsWeightInfo = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type Balance = u64;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxFreezes = ();
    type RuntimeFreezeReason = ();
    type RuntimeHoldReason = ();
    type DoneSlashHandler = ();
}

impl pallet_ai_predictions::Config for Test {
    type Currency = Balances;
    type PalletId = AiPredictionsPalletId;
    type MaxHashLen = ConstU32<128>;
    type MaxMetadataUriLen = ConstU32<256>;
    type MaxCategoryCodeLen = ConstU32<96>;
    type MaxModelsPerOwner = ConstU32<64>;
    type MaxPredictionsPerModel = ConstU32<256>;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("Genesis storage should build");

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 1_000_000), (BOB, 1_000_000), (VALIDATOR, 1_000_000)],
        ..Default::default()
    }
    .assimilate_storage(&mut storage)
    .expect("Balances genesis should assimilate");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        crate::ModelOnboardingConfig::<Test>::put(crate::ModelOnboardingSettings {
            public_model_registration_enabled: true,
            require_model_approval: false,
            ..Default::default()
        });
    });
    ext
}

pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;
pub const VALIDATOR: u64 = 3;
