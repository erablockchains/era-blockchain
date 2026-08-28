//! RPC wiring for full node.
//!
//! Notes for this SDK version:
//! - `substrate_frame_rpc_system::System::new(client, pool)` takes **only 2 args**.
//! - To call `.into_rpc()` you must import the corresponding `*ApiServer` traits.
//! - Use `sc_transaction_pool_api::TransactionPool` (the *API* crate) for bounds.
//! - Convert `jsonrpsee` merge errors into `sc_service::Error`.

use std::sync::Arc;

use jsonrpsee::RpcModule;
use sc_client_api::{AuxStore, HeaderBackend};
use sc_service::Error as ServiceError;
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;

use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
use pallet_transaction_payment_rpc_runtime_api as tp_runtime_api;
use substrate_frame_rpc_system::{System, SystemApiServer};

use era_runtime::{opaque::Block, AccountId, Balance, Index};

/// Compose the full RPC module for the node.
///
/// `C` - client type (must provide the runtime APIs used by RPCs)
/// `P` - transaction pool handle (any type implementing the TxPool trait)
pub fn create_full<C, P>(client: Arc<C>, pool: Arc<P>) -> Result<RpcModule<()>, Box<ServiceError>>
where
    C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + AuxStore + Send + Sync + 'static,
    // System RPC needs nonce + block builder runtime APIs
    C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Index>
        + tp_runtime_api::TransactionPaymentApi<Block, Balance>
        + BlockBuilder<Block>,
    P: TransactionPool<Block = Block> + Send + Sync + 'static,
{
    let mut module = RpcModule::new(());

    // System RPC (account nonce, chain info, etc.)
    let sys_rpc = System::new(client.clone(), pool.clone()).into_rpc();
    module
        .merge(sys_rpc)
        .map_err(|e| ServiceError::Other(e.to_string()))?;

    // Transaction Payment RPC (fee/weight queries)
    let txpay_rpc = TransactionPayment::new(client.clone()).into_rpc();
    module
        .merge(txpay_rpc)
        .map_err(|e| ServiceError::Other(e.to_string()))?;

    Ok(module)
}
