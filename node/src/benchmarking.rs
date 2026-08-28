#![cfg(feature = "runtime-benchmarks")]

use era_runtime as runtime;
use sp_keyring::Sr25519Keyring;

use frame_benchmarking_cli::ExtrinsicBuilder;

// Minimal examples — keep your previous builders if you had them
pub struct RemarkBuilder;
impl ExtrinsicBuilder for RemarkBuilder {
    type Config = ();
    fn pallet(&self) -> &str { "System" }
    fn extrinsic(&self) -> &str { "remark" }
    fn build(&self, _cfg: &Self::Config, _nonce: u32) -> runtime::UncheckedExtrinsic {
        // Fill with a harmless no-op remark; replace with your desired call
        use runtime::RuntimeCall;
        let call = RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() });
        runtime::UncheckedExtrinsic::new_unsigned(call)
    }
}

