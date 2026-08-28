use std::sync::Arc;

use futures::future::BoxFuture;
use jsonrpsee::RpcModule;

use sc_basic_authorship::ProposerFactory;
use sc_client_api::BlockBackend;
use sc_consensus::LongestChain;
use sc_network::config::FullNetworkConfiguration;
use sc_network::{NetworkBackend, NetworkWorker};
use sc_service::{
    build_network, error::Error as ServiceError, BuildNetworkParams, Configuration,
    PartialComponents, SpawnTaskHandle, TFullBackend, TFullClient, TaskManager, WarpSyncConfig,
};
use sc_transaction_pool::TransactionPoolHandle;
use sc_transaction_pool_api::OffchainTransactionPoolFactory;

use era_runtime::opaque::Block;
use sp_runtime::traits::Block as BlockT;

// BABE / GRANDPA
use sc_consensus_grandpa::{self, GenesisAuthoritySetProvider, SharedVoterState};
use sp_consensus_babe::inherents::InherentDataProvider as BabeInherentProvider;
use sp_core::traits::{SpawnEssentialNamed, SpawnNamed};

pub type FullClient = TFullClient<
    Block,
    era_runtime::RuntimeApi,
    sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
>;
pub type FullBackend = TFullBackend<Block>;
pub type SelectChain = LongestChain<FullBackend, Block>;

// Use the same handle type as the v1.19 solochain template.
type ExPool = TransactionPoolHandle<Block, FullClient>;
type ServiceResult<T> = Result<T, Box<ServiceError>>;
type RpcBuilder =
    Box<dyn Fn(Arc<dyn SpawnNamed>) -> Result<RpcModule<()>, ServiceError> + Send + Sync>;

fn boxed_service_error(error: impl Into<ServiceError>) -> Box<ServiceError> {
    Box::new(error.into())
}

/// Wrapper that implements `SpawnNamed` + `SpawnEssentialNamed` for a non-essential
/// spawner, by delegating both to `SpawnTaskHandle`.
#[derive(Clone)]
struct NonEssentialSpawner(SpawnTaskHandle);

impl SpawnNamed for NonEssentialSpawner {
    fn spawn(&self, name: &'static str, group: Option<&'static str>, fut: BoxFuture<'static, ()>) {
        // non-essential task
        self.0.spawn(name, group, fut);
    }

    fn spawn_blocking(
        &self,
        name: &'static str,
        group: Option<&'static str>,
        fut: BoxFuture<'static, ()>,
    ) {
        // "blocking" but still non-essential from BABE’s POV
        self.0.spawn_blocking(name, group, fut);
    }
}

impl SpawnEssentialNamed for NonEssentialSpawner {
    fn spawn_essential(
        &self,
        name: &'static str,
        group: Option<&'static str>,
        fut: BoxFuture<'static, ()>,
    ) {
        self.0.spawn_blocking(name, group, fut);
    }

    fn spawn_essential_blocking(
        &self,
        name: &'static str,
        group: Option<&'static str>,
        fut: BoxFuture<'static, ()>,
    ) {
        self.0.spawn_blocking(name, group, fut);
    }
}

/// Build the partial components used by the node (everything except the import-queue).
pub fn new_partial(
    config: &Configuration,
) -> ServiceResult<
    PartialComponents<
        FullClient,
        FullBackend,
        SelectChain,
        (),     // import_queue prepared later
        ExPool, // transaction pool handle
        (),
    >,
> {
    let executor = sc_service::new_wasm_executor::<sp_io::SubstrateHostFunctions>(&config.executor);

    let (client, backend, keystore_container, task_manager) =
        sc_service::new_full_parts::<Block, era_runtime::RuntimeApi, _>(config, None, executor)
            .map_err(boxed_service_error)?;
    let client = Arc::new(client);

    let select_chain = LongestChain::new(backend.clone());

    // Transaction pool: same pattern as solochain template (Builder + handle).
    let transaction_pool = Arc::from(
        sc_transaction_pool::Builder::new(
            task_manager.spawn_essential_handle(),
            client.clone(),
            config.role.is_authority().into(),
        )
        .with_options(config.transaction_pool.clone())
        .with_prometheus(config.prometheus_registry())
        .build(),
    );

    Ok(PartialComponents {
        client,
        backend,
        task_manager,
        select_chain,
        import_queue: (),
        transaction_pool,
        other: (),
        keystore_container,
    })
}

/// Build and start the full service (node).
pub fn new_full(mut config: Configuration) -> ServiceResult<(TaskManager, Arc<FullClient>)> {
    let PartialComponents {
        client,
        backend,
        mut task_manager,
        select_chain,
        transaction_pool: tx_pool, // ExPool (handle)
        keystore_container,
        ..
    } = new_partial(&config)?;

    // --- GRANDPA block import + link ---
    let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
        client.clone(),
        512u32, // justification_generation_period (matches template constant)
        &client as &dyn GenesisAuthoritySetProvider<Block>,
        select_chain.clone(),
        None,
    )
    .map_err(boxed_service_error)?;

    // --- BABE configuration from runtime + block import / link ---
    let babe_cfg = sc_consensus_babe::configuration(&*client).map_err(boxed_service_error)?;
    let (babe_block_import, babe_link) = sc_consensus_babe::block_import(
        babe_cfg.clone(),
        grandpa_block_import.clone(),
        client.clone(),
    )
    .map_err(boxed_service_error)?;

    let slot_duration = babe_cfg.slot_duration();

    // --- BABE import queue ---
    let non_essential_spawner = NonEssentialSpawner(task_manager.spawn_handle());

    let (import_queue, _babe_worker_handle) =
        sc_consensus_babe::import_queue::<Block, FullClient, SelectChain, _, _, _>(
            sc_consensus_babe::ImportQueueParams {
                link: babe_link.clone(),
                block_import: babe_block_import.clone(),
                justification_import: Some(Box::new(grandpa_block_import.clone())),
                registry: config.prometheus_registry(),
                spawner: &non_essential_spawner, // satisfies SpawnEssentialNamed
                client: client.clone(),
                select_chain: select_chain.clone(),
                create_inherent_data_providers: move |_, ()| {
                    let slot_duration = slot_duration;
                    async move {
                        // Timestamp from system time
                        let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

                        // BABE slot from timestamp + slot duration
                        let slot = BabeInherentProvider::from_timestamp_and_slot_duration(
                            *timestamp,
                            slot_duration,
                        );

                        // v1.19 order: (slot, timestamp)
                        Ok((slot, timestamp))
                    }
                },
                offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(tx_pool.clone()),
                telemetry: None,
            },
        )
        .map_err(boxed_service_error)?;

    // --- Networking (GRANDPA-aware, like solochain template) ---
    let prometheus_registry = config.prometheus_registry().cloned();
    type NetBackend = NetworkWorker<Block, <Block as BlockT>::Hash>;

    let mut net_config: FullNetworkConfiguration<Block, <Block as BlockT>::Hash, NetBackend> =
        FullNetworkConfiguration::new(&config.network, prometheus_registry.clone());

    // GRANDPA-specific notification metrics
    let metrics = NetBackend::register_notification_metrics(config.prometheus_registry());

    // GRANDPA peers set + notification service
    let peer_store_handle = net_config.peer_store_handle();
    let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
        &client
            .block_hash(0)
            .ok()
            .flatten()
            .expect("Genesis block exists; qed"),
        &config.chain_spec,
    );
    let (grandpa_protocol_config, grandpa_notification_service) =
        sc_consensus_grandpa::grandpa_peers_set_config::<_, NetBackend>(
            grandpa_protocol_name.clone(),
            metrics.clone(),
            peer_store_handle,
        );
    net_config.add_notification_protocol(grandpa_protocol_config);

    // GRANDPA warp sync provider
    let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
        backend.clone(),
        grandpa_link.shared_authority_set().clone(),
        Vec::new(),
    ));

    let (network, system_rpc_tx, tx_handler_controller, sync_service) =
        build_network(BuildNetworkParams {
            config: &mut config,
            net_config,
            client: client.clone(),
            transaction_pool: tx_pool.clone(),
            spawn_handle: task_manager.spawn_handle(),
            import_queue,
            block_announce_validator_builder: None,
            warp_sync_config: Some(WarpSyncConfig::WithProvider(warp_sync)),
            block_relay: None,
            metrics,
        })
        .map_err(boxed_service_error)?;

    // --- Cache values from `config` before it’s moved into spawn_tasks ---
    let role = config.role;
    let force_authoring = config.force_authoring;
    let backoff_authoring_blocks: Option<()> = None;
    let enable_grandpa = !config.disable_grandpa;
    let node_name = config.network.node_name.clone();

    // --- RPC ---
    let rpc_client = client.clone();
    let rpc_pool = tx_pool.clone();
    let rpc_builder: RpcBuilder = Box::new(move |_spawn| {
        crate::rpc::create_full(rpc_client.clone(), rpc_pool.clone()).map_err(|error| *error)
    });

    // --- Spawn core tasks ---
    sc_service::spawn_tasks(sc_service::SpawnTasksParams {
        network: Arc::new(network.clone()),
        client: client.clone(),
        transaction_pool: tx_pool.clone(),
        task_manager: &mut task_manager,
        rpc_builder,
        system_rpc_tx,
        tx_handler_controller,
        config,
        backend: backend.clone(),
        keystore: keystore_container.keystore(),
        telemetry: None,
        sync_service: sync_service.clone(),
    })
    .map_err(boxed_service_error)?;

    // --- Start BABE worker (only if authority) ---
    if role.is_authority() {
        log::info!("🔧 Role=AUTHORITY: starting BABE authoring");

        let proposer = ProposerFactory::new(
            task_manager.spawn_handle(),
            client.clone(),
            tx_pool.clone(),
            None,
            None,
        );

        let babe_worker = sc_consensus_babe::start_babe(sc_consensus_babe::BabeParams {
            keystore: keystore_container.keystore(),
            client: client.clone(),
            select_chain: select_chain.clone(),
            env: proposer,
            block_import: babe_block_import.clone(),
            sync_oracle: sync_service.clone(),
            justification_sync_link: sync_service.clone(),
            create_inherent_data_providers: move |_, ()| {
                let slot_duration = slot_duration;
                async move {
                    let timestamp = sp_timestamp::InherentDataProvider::from_system_time();
                    let slot = BabeInherentProvider::from_timestamp_and_slot_duration(
                        *timestamp,
                        slot_duration,
                    );
                    Ok((slot, timestamp))
                }
            },
            force_authoring,
            backoff_authoring_blocks,
            babe_link,
            telemetry: None,
            block_proposal_slot_portion: sc_consensus_babe::SlotProportion::new(0.5),
            max_block_proposal_slot_portion: None,
        })
        .map_err(boxed_service_error)?;

        task_manager
            .spawn_essential_handle()
            .spawn_blocking("babe-worker", None, babe_worker);
    } else {
        log::info!("👀 Role=FULL: BABE authoring disabled (observer node).");
    }

    // --- GRANDPA voter (same pattern as solochain template) ---
    if enable_grandpa {
        let keystore = if role.is_authority() {
            Some(keystore_container.keystore())
        } else {
            None
        };

        let grandpa_config = sc_consensus_grandpa::Config {
            gossip_duration: std::time::Duration::from_millis(333),
            justification_generation_period: 512,
            name: Some(node_name),
            observer_enabled: false,
            keystore,
            local_role: role,
            telemetry: None,
            protocol_name: grandpa_protocol_name,
        };

        let grandpa_params = sc_consensus_grandpa::GrandpaParams {
            config: grandpa_config,
            link: grandpa_link,
            network,
            sync: Arc::new(sync_service),
            notification_service: grandpa_notification_service,
            voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
            prometheus_registry,
            shared_voter_state: SharedVoterState::empty(),
            telemetry: None,
            offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(tx_pool.clone()),
        };

        task_manager.spawn_essential_handle().spawn_blocking(
            "grandpa-voter",
            None,
            sc_consensus_grandpa::run_grandpa_voter(grandpa_params).map_err(boxed_service_error)?,
        );
    }

    Ok((task_manager, client))
}

#[cfg(test)]
mod tests {
    #[test]
    fn supported_wasm_executor_accepts_default_service_configuration() {
        let config = sc_service::config::ExecutorConfiguration::default();
        let _: sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions> =
            sc_service::new_wasm_executor(&config);
    }

    #[test]
    fn supported_wasm_executor_accepts_explicit_service_limits() {
        let config = sc_service::config::ExecutorConfiguration {
            max_runtime_instances: 3,
            default_heap_pages: Some(64),
            runtime_cache_size: 1,
            ..Default::default()
        };
        let _: sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions> =
            sc_service::new_wasm_executor(&config);
    }
}
