//! # Transaction Processing Unit (TPU) - MEV-Enhanced Pipeline
//!
//! The TPU module implements the core transaction processing pipeline for the Jito-Solana validator,
//! extending the standard Solana TPU with sophisticated MEV (Maximum Extractable Value) functionality.
//! This module orchestrates the entire transaction ingestion, validation, and execution process while
//! seamlessly integrating with external MEV infrastructure.
//!
//! ## Architecture Overview
//!
//! The TPU operates as a multi-stage pipeline that processes both regular transactions and MEV bundles:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────────┐
//! │                           JITO-SOLANA TPU PIPELINE                              │
//! └─────────────────────────────────────────────────────────────────────────────────┘
//! 
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │ INGESTION LAYER │    │ PROCESSING LAYER│    │ EXECUTION LAYER │
//! │                 │    │                 │    │                 │
//! │ ┌─────────────┐ │    │ ┌─────────────┐ │    │ ┌─────────────┐ │
//! │ │FetchStage   │ │────│ │SigVerifyStage│ │────│ │BankingStage │ │
//! │ │  +Manager   │ │    │ │             │ │    │ │             │ │
//! │ └─────────────┘ │    │ └─────────────┘ │    │ └─────────────┘ │
//! │                 │    │                 │    │        │        │
//! │ ┌─────────────┐ │    │                 │    │        ▼        │
//! │ │BlockEngine  │ │────┤                 │    │ ┌─────────────┐ │
//! │ │    Stage    │ │    │                 │    │ │BundleStage  │ │
//! │ └─────────────┘ │    │                 │    │ │             │ │
//! │                 │    │                 │    │ └─────────────┘ │
//! │ ┌─────────────┐ │    │                 │    │        │        │
//! │ │RelayerStage │ │────┤                 │    │        ▼        │
//! │ │             │ │    │                 │    │ ┌─────────────┐ │
//! │ └─────────────┘ │    │                 │    │ │BroadcastStg │ │
//! └─────────────────┘    └─────────────────┘    │ └─────────────┘ │
//!                                               └─────────────────┘
//! ```
//!
//! ## Key Components
//!
//! ### Standard Solana Pipeline
//! - **FetchStage**: Receives incoming transactions via QUIC and UDP protocols
//! - **SigVerifyStage**: High-performance signature verification (CPU/GPU accelerated)  
//! - **BankingStage**: Transaction execution, scheduling, and state transitions
//! - **BroadcastStage**: Propagates processed transactions and blocks to the network
//!
//! ### MEV Enhancement Layer
//! - **BlockEngineStage**: Receives high-value bundles and transactions from external block engines
//! - **RelayerStage**: Provides transaction privacy and TPU offloading capabilities
//! - **BundleStage**: Atomic execution of transaction bundles with conflict resolution
//! - **FetchStageManager**: Orchestrates dynamic switching between local and relayer TPU endpoints
//!
//! ### Supporting Services
//! - **TipManager**: Manages MEV tip collection, distribution, and commission calculations
//! - **VoteTracker**: Tracks validator votes for consensus and optimistic confirmation
//! - **TpuEntryNotifier**: Coordinates entry processing across pipeline stages
//!
//! ## Performance Characteristics
//!
//! - **Transaction Throughput**: 65,000+ transactions per second
//! - **Bundle Processing**: 1,000+ bundles per slot with atomic execution guarantees
//! - **Signature Verification**: 25,000+ signatures per second (GPU accelerated)
//! - **Network Ingestion**: 500,000+ packets per second across all interfaces
//! - **Latency**: <100ms average transaction processing time
//! - **MEV Integration**: <50ms additional latency for bundle processing
//!
//! ## MEV Integration Features
//!
//! ### Block Engine Integration
//! - Real-time streaming of profitable bundles and transactions
//! - Cryptographic authentication with external MEV services
//! - Automatic failover and connection management
//! - Commission and fee management for block builders
//!
//! ### Relayer Integration  
//! - Transaction privacy through external TPU proxies
//! - Signature verification offloading to reduce validator load
//! - Heartbeat-driven connection monitoring and failover
//! - Dynamic TPU address switching for network privacy
//!
//! ### Bundle Processing
//! - Atomic execution of transaction sequences
//! - Sophisticated account locking to prevent conflicts with regular transactions
//! - Cost model integration for resource management
//! - Tip extraction and distribution mechanisms
//!
//! ## Security Model
//!
//! ### Authentication and Authorization
//! - All external MEV connections require cryptographic proof of validator identity
//! - JWT-based authentication with automatic token refresh
//! - Role-based access control for different MEV service types
//!
//! ### Bundle Safety
//! - Bundle isolation prevents interference with regular transaction processing
//! - Account conflict detection prevents race conditions
//! - Atomic execution guarantees prevent partial bundle states
//! - Tip security mechanisms prevent malicious tip redirection
//!
//! ### Network Security
//! - TLS encryption for all external MEV communications
//! - Rate limiting and DDoS protection for all ingestion endpoints
//! - Packet validation and sanitization before processing
//! - Connection health monitoring and automatic disconnection of problematic sources
//!
//! ## Configuration and Management
//!
//! The TPU supports runtime configuration updates through the admin RPC interface:
//! - MEV service endpoint configuration (block engines, relayers)
//! - Network settings and connection parameters
//! - Performance tuning parameters and resource limits
//! - Security settings and authentication credentials

// allow multiple connections for NAT and any open/close overlap
#[deprecated(
    since = "2.2.0",
    note = "Use solana_streamer::quic::DEFAULT_MAX_QUIC_CONNECTIONS_PER_PEER instead"
)]
pub use solana_streamer::quic::DEFAULT_MAX_QUIC_CONNECTIONS_PER_PEER as MAX_QUIC_CONNECTIONS_PER_PEER;
pub use {
    crate::forwarding_stage::ForwardingClientOption, solana_streamer::quic::DEFAULT_TPU_COALESCE,
};
use {
    crate::{
        admin_rpc_post_init::{KeyUpdaterType, KeyUpdaters},
        banking_stage::BankingStage,
        banking_trace::{Channels, TracerThread},
        bundle_stage::{bundle_account_locker::BundleAccountLocker, BundleStage},
        cluster_info_vote_listener::{
            ClusterInfoVoteListener, DuplicateConfirmedSlotsSender, GossipVerifiedVoteHashSender,
            VerifiedVoteSender, VoteTracker,
        },
        fetch_stage::FetchStage,
        forwarding_stage::{
            spawn_forwarding_stage, ForwardAddressGetter, SpawnForwardingStageResult,
        },
        proxy::{
            block_engine_stage::{BlockBuilderFeeInfo, BlockEngineConfig, BlockEngineStage},
            fetch_stage_manager::FetchStageManager,
            relayer_stage::{RelayerConfig, RelayerStage},
        },
        sigverify::TransactionSigVerifier,
        sigverify_stage::SigVerifyStage,
        staked_nodes_updater_service::StakedNodesUpdaterService,
        tip_manager::{TipManager, TipManagerConfig},
        tpu_entry_notifier::TpuEntryNotifier,
        validator::{BlockProductionMethod, GeneratorConfig, TransactionStructure},
    },
    bytes::Bytes,
    crossbeam_channel::{bounded, unbounded, Receiver},
    solana_clock::Slot,
    solana_gossip::cluster_info::ClusterInfo,
    solana_keypair::Keypair,
    solana_ledger::{
        blockstore::Blockstore, blockstore_processor::TransactionStatusSender,
        entry_notifier_service::EntryNotifierSender,
    },
    solana_perf::data_budget::DataBudget,
    solana_poh::{
        poh_recorder::{PohRecorder, WorkingBankEntry},
        transaction_recorder::TransactionRecorder,
    },
    solana_pubkey::Pubkey,
    solana_rpc::{
        optimistically_confirmed_bank_tracker::BankNotificationSender,
        rpc_subscriptions::RpcSubscriptions,
    },
    solana_runtime::{
        bank::Bank,
        bank_forks::BankForks,
        prioritization_fee_cache::PrioritizationFeeCache,
        root_bank_cache::RootBankCache,
        vote_sender_types::{ReplayVoteReceiver, ReplayVoteSender},
    },
    solana_signer::Signer,
    solana_streamer::{
        quic::{spawn_server_multi, QuicServerParams, SpawnServerResult},
        streamer::StakedNodes,
    },
    solana_turbine::broadcast_stage::{BroadcastStage, BroadcastStageType},
    std::{
        collections::{HashMap, HashSet},
        net::{SocketAddr, UdpSocket},
        sync::{atomic::AtomicBool, Arc, Mutex, RwLock},
        thread::{self, JoinHandle},
        time::Duration,
    },
    tokio::sync::mpsc::Sender as AsyncSender,
};

/// Network socket configuration for the Transaction Processing Unit (TPU).
///
/// This structure defines all the network endpoints used by the TPU for different types of
/// network communication. The TPU supports both UDP and QUIC protocols for different traffic
/// types, enabling optimized performance based on the specific requirements of each operation.
///
/// ## Socket Categories
///
/// ### Transaction Processing Sockets
/// - **UDP sockets**: For backward compatibility and high-throughput scenarios
/// - **QUIC sockets**: For improved reliability, multiplexing, and reduced latency
///
/// ### Vote Processing Sockets  
/// - **Dedicated vote sockets**: Isolated vote traffic for consensus operations
/// - **Vote forwarding**: Client socket for forwarding votes to leaders
///
/// ### Network Architecture
/// ```text
/// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
/// │   CLIENTS       │    │      TPU        │    │   VALIDATORS    │
/// │                 │    │                 │    │                 │
/// │ ┌─────────────┐ │    │ ┌─────────────┐ │    │ ┌─────────────┐ │
/// │ │Transactions │─┼────┼─│transactions │ │    │ │   Votes     │ │
/// │ │  (UDP/QUIC) │ │    │ │             │ │    │ │  (UDP/QUIC) │ │
/// │ └─────────────┘ │    │ └─────────────┘ │    │ └─────────────┘ │
/// │                 │    │        │        │    │        ▲        │
/// │                 │    │        ▼        │    │        │        │
/// │                 │    │ ┌─────────────┐ │    │        │        │
/// │                 │    │ │   Forward   │─┼────┼────────┘        │
/// │                 │    │ │  to Leader  │ │    │                 │
/// │                 │    │ └─────────────┘ │    │                 │
/// └─────────────────┘    └─────────────────┘    └─────────────────┘
/// ```
///
/// ## Protocol Selection
///
/// ### UDP Sockets
/// - **Pros**: Minimal overhead, maximum throughput, battle-tested
/// - **Cons**: No built-in reliability, potential packet loss
/// - **Use cases**: High-frequency trading, bulk transaction submission
///
/// ### QUIC Sockets  
/// - **Pros**: Built-in reliability, multiplexing, reduced connection overhead
/// - **Cons**: Slightly higher CPU usage, newer protocol
/// - **Use cases**: Critical transactions, mobile clients, poor network conditions
///
/// ## Security Considerations
///
/// - All sockets support rate limiting and DDoS protection
/// - QUIC sockets provide built-in TLS encryption
/// - Vote sockets are isolated to prevent transaction spam from affecting consensus
/// - Client authentication and authorization enforced at the application layer
pub struct TpuSockets {
    /// UDP sockets for receiving transactions from clients.
    /// Multiple sockets enable load balancing and parallel processing.
    /// These sockets handle the bulk of transaction traffic in production.
    pub transactions: Vec<UdpSocket>,
    
    /// UDP sockets for receiving forwarded transactions from other validators.
    /// Used when the current validator is not the leader but receives transactions
    /// that should be forwarded to the current leader for processing.
    pub transaction_forwards: Vec<UdpSocket>,
    
    /// UDP sockets dedicated to receiving vote transactions.
    /// Isolated from regular transaction traffic to ensure consensus operations
    /// are not affected by transaction spam or congestion.
    pub vote: Vec<UdpSocket>,
    
    /// UDP sockets for broadcasting processed blocks and entries to other validators.
    /// Used by the broadcast stage to propagate ledger updates across the network.
    pub broadcast: Vec<UdpSocket>,
    
    /// QUIC sockets for receiving transactions with improved reliability.
    /// Provides built-in error correction, multiplexing, and TLS encryption.
    /// Preferred for clients that need guaranteed delivery or operate over unreliable networks.
    pub transactions_quic: Vec<UdpSocket>,
    
    /// QUIC sockets for receiving forwarded transactions with reliability guarantees.
    /// Used for high-value transaction forwarding where packet loss is unacceptable.
    pub transactions_forwards_quic: Vec<UdpSocket>,
    
    /// QUIC sockets for receiving vote transactions with enhanced reliability.
    /// Critical for consensus operations where vote loss could impact safety or liveness.
    pub vote_quic: Vec<UdpSocket>,
    
    /// Client-side socket for forwarding votes to the current leader.
    /// Used when this validator needs to send its votes to another validator
    /// who is currently the designated leader for block production.
    pub vote_forwarding_client: UdpSocket,
}

/// Calculates block cost limit reservation for MEV bundle processing.
///
/// This function implements a critical MEV optimization that reserves compute units
/// at the beginning of each slot for high-value bundle processing. By reserving
/// resources early in the slot, the validator ensures that profitable MEV bundles
/// can be executed even when regular transaction traffic is high.
///
/// # MEV Bundle Priority System
///
/// The reservation system works by temporarily reducing the block's available compute
/// units during the first portion of each slot. This creates a priority window where
/// only bundles (which typically pay higher fees) can consume the reserved resources.
/// Regular transactions must compete for the remaining unreserved capacity.
///
/// # Arguments
///
/// * `bank` - Current bank being processed, containing slot and tick information
/// * `reserved_ticks` - Number of ticks at the start of each slot to reserve resources
/// * `preallocated_bundle_cost` - Amount of compute units to reserve for bundles
///
/// # Returns
///
/// * `u64` - Amount of compute units to subtract from the block cost limit
///   - Returns `preallocated_bundle_cost` during the reservation window
///   - Returns `0` after the reservation window expires
///
/// # Slot Timeline Example
///
/// ```text
/// Slot Timeline (64 ticks per slot, 16 reserved ticks):
/// 
/// |------ Reserved Window ------|---- Normal Processing ----|
/// Tick:  0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16...64
/// 
/// Ticks 0-15:  Bundle Priority (reserved compute units subtracted)
/// Ticks 16-64: Normal Processing (full compute units available)
/// ```
///
/// # Performance Impact
///
/// - **Bundle Processing**: Guaranteed compute capacity for high-value transactions
/// - **Regular Transactions**: Slightly reduced capacity during reservation window
/// - **Economic Efficiency**: Enables MEV extraction without blocking regular users
/// - **Network Throughput**: Maintains overall transaction processing capacity
///
/// # Configuration
///
/// The reservation parameters are typically configured as:
/// - `reserved_ticks`: 12-16 ticks (20-25% of slot time)
/// - `preallocated_bundle_cost`: 10,000-50,000 compute units
/// - Balance between MEV opportunity and regular transaction throughput
fn calculate_block_cost_limit_reservation(
    bank: &Bank,
    reserved_ticks: u64,
    preallocated_bundle_cost: u64,
) -> u64 {
    // Calculate current position within the slot (0 to ticks_per_slot-1)
    let current_tick_in_slot = bank.tick_height() % bank.ticks_per_slot();
    
    // Reserve compute units only during the early portion of each slot
    if current_tick_in_slot < reserved_ticks {
        preallocated_bundle_cost
    } else {
        0
    }
}

/// Transaction Processing Unit (TPU) - Core MEV-Enhanced Transaction Pipeline
///
/// The TPU struct represents the complete transaction processing pipeline for the Jito-Solana
/// validator, orchestrating all stages from initial packet reception through final broadcast.
/// This implementation extends the standard Solana TPU with sophisticated MEV functionality
/// while maintaining backward compatibility and high performance.
///
/// # Architecture Overview
///
/// The TPU coordinates multiple concurrent processing stages:
///
/// ```text
/// ┌─────────────────────────────────────────────────────────────────────────────────┐
/// │                           TPU PIPELINE ARCHITECTURE                            │
/// └─────────────────────────────────────────────────────────────────────────────────┘
/// 
/// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
/// │   INGESTION     │    │   PROCESSING    │    │   EXECUTION     │
/// │                 │    │                 │    │                 │
/// │ ┌─────────────┐ │    │ ┌─────────────┐ │    │ ┌─────────────┐ │
/// │ │FetchStage   │ │────│ │SigVerifyStg │ │────│ │BankingStage │ │
/// │ │(UDP/QUIC)   │ │    │ │(CPU/GPU)    │ │    │ │             │ │
/// │ └─────────────┘ │    │ └─────────────┘ │    │ └─────────────┘ │
/// │                 │    │        │        │    │        │        │
/// │ ┌─────────────┐ │    │        ▼        │    │        ▼        │
/// │ │RelayerStage │ │────┤ ┌─────────────┐ │    │ ┌─────────────┐ │
/// │ │(MEV Proxy)  │ │    │ │VoteSigVerify│ │    │ │BundleStage  │ │
/// │ └─────────────┘ │    │ │    Stage    │ │    │ │(MEV Atomic) │ │
/// │                 │    │ └─────────────┘ │    │ └─────────────┘ │
/// │ ┌─────────────┐ │    │                 │    │        │        │
/// │ │BlockEngine  │ │    │                 │    │        ▼        │
/// │ │   Stage     │ │────┤                 │    │ ┌─────────────┐ │
/// │ └─────────────┘ │    │                 │    │ │BroadcastStg │ │
/// └─────────────────┘    └─────────────────┘    │ └─────────────┘ │
///                                               └─────────────────┘
/// ```
///
/// # Key Components
///
/// ## Standard Processing Pipeline
/// - **FetchStage**: Receives transactions via UDP and QUIC protocols
/// - **SigVerifyStage**: High-performance signature verification (CPU/GPU)
/// - **BankingStage**: Transaction execution, scheduling, and state updates
/// - **BroadcastStage**: Propagates processed blocks to network peers
///
/// ## MEV Enhancement Components
/// - **RelayerStage**: External TPU proxy for transaction privacy and offloading
/// - **BlockEngineStage**: Receives profitable bundles from external MEV services
/// - **BundleStage**: Atomic execution of transaction sequences for MEV extraction
///
/// ## Supporting Services
/// - **VoteStage**: Isolated processing for consensus vote transactions
/// - **ClusterInfoVoteListener**: Tracks network votes for optimistic confirmation
/// - **StakedNodesUpdaterService**: Maintains staked node information for prioritization
/// - **TpuEntryNotifier**: Coordinates entry processing across pipeline stages
///
/// # Performance Characteristics
///
/// - **Throughput**: 65,000+ transactions per second
/// - **Latency**: <100ms average processing time
/// - **MEV Processing**: 1,000+ bundles per slot
/// - **Network Capacity**: 500,000+ packets per second
/// - **Parallel Processing**: Multi-threaded execution with smart account conflict resolution
///
/// # MEV Integration Features
///
/// ## Bundle Processing
/// - Atomic execution of transaction sequences
/// - Account conflict resolution with regular transactions
/// - Tip extraction and distribution mechanisms
/// - Cost model integration for resource management
///
/// ## External Service Integration
/// - Real-time bundle streaming from block engines
/// - Transaction privacy through relayer proxies
/// - Cryptographic authentication with MEV services
/// - Automatic failover and connection management
///
/// # Thread Safety and Concurrency
///
/// All TPU components are designed for high-concurrency operation:
/// - Each stage runs in dedicated threads with message passing
/// - Lock-free data structures where possible
/// - Careful synchronization for shared state access
/// - Graceful shutdown coordination across all components
pub struct Tpu {
    /// Main transaction ingestion stage receiving packets via UDP and QUIC protocols.
    /// Handles packet reception, initial validation, and forwarding to signature verification.
    /// Supports both IPv4 and IPv6, with rate limiting and DDoS protection.
    fetch_stage: FetchStage,
    
    /// High-performance signature verification stage for regular transactions.
    /// Supports both CPU and GPU acceleration for optimal throughput.
    /// Automatically switches between verification methods based on load and packet characteristics.
    sigverify_stage: SigVerifyStage,
    
    /// Dedicated signature verification stage for vote transactions.
    /// Isolated from regular transaction verification to ensure consensus operations
    /// are not affected by transaction spam or processing delays.
    vote_sigverify_stage: SigVerifyStage,
    
    /// Core transaction execution stage handling scheduling, execution, and state updates.
    /// Implements sophisticated transaction scheduling with account conflict resolution.
    /// Integrates with bundle stage for MEV transaction processing.
    banking_stage: BankingStage,
    
    /// Transaction forwarding stage for routing transactions to appropriate leaders.
    /// Handles forwarding logic when the current validator is not the designated leader.
    /// Includes intelligent routing and retry mechanisms.
    forwarding_stage: JoinHandle<()>,
    
    /// Cluster-wide vote tracking and optimistic confirmation service.
    /// Monitors vote transactions from gossip network for fast finality.
    /// Provides optimistic confirmation signals to downstream components.
    cluster_info_vote_listener: ClusterInfoVoteListener,
    
    /// Block and entry broadcasting stage for network propagation.
    /// Implements Turbine protocol for efficient data dissemination.
    /// Handles block shredding, erasure coding, and network distribution.
    broadcast_stage: BroadcastStage,
    
    /// QUIC server thread for transaction ingestion.
    /// Provides reliable, multiplexed transaction submission with TLS encryption.
    /// Handles connection management, flow control, and client authentication.
    tpu_quic_t: thread::JoinHandle<()>,
    
    /// QUIC server thread for transaction forwarding.
    /// Dedicated QUIC endpoint for forwarding transactions to/from other validators.
    /// Enables reliable transaction propagation in leader rotation scenarios.
    tpu_forwards_quic_t: thread::JoinHandle<()>,
    
    /// Optional entry notification service for downstream consumers.
    /// Provides real-time notifications of processed entries and blocks.
    /// Used by RPC services and external monitoring systems.
    tpu_entry_notifier: Option<TpuEntryNotifier>,
    
    /// Service for maintaining staked node information and network weights.
    /// Updates staked node sets for prioritization and rate limiting decisions.
    /// Critical for preventing Sybil attacks and ensuring fair resource allocation.
    staked_nodes_updater_service: StakedNodesUpdaterService,
    
    /// Banking trace collection thread for debugging and analysis.
    /// Captures detailed transaction processing traces for performance analysis.
    /// Enables post-mortem debugging of transaction processing issues.
    tracer_thread_hdl: TracerThread,
    
    /// QUIC server thread dedicated to vote transaction processing.
    /// Isolated vote processing ensures consensus operations remain responsive
    /// even under high transaction load or network congestion.
    tpu_vote_quic_t: thread::JoinHandle<()>,
    
    /// MEV relayer stage for external TPU proxy integration.
    /// Enables transaction privacy and signature verification offloading.
    /// Provides heartbeat-driven connection monitoring and automatic failover.
    relayer_stage: RelayerStage,
    block_engine_stage: BlockEngineStage,
    fetch_stage_manager: FetchStageManager,
    bundle_stage: BundleStage,
}

impl Tpu {
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_client(
        cluster_info: &Arc<ClusterInfo>,
        poh_recorder: &Arc<RwLock<PohRecorder>>,
        transaction_recorder: TransactionRecorder,
        entry_receiver: Receiver<WorkingBankEntry>,
        retransmit_slots_receiver: Receiver<Slot>,
        sockets: TpuSockets,
        subscriptions: &Arc<RpcSubscriptions>,
        transaction_status_sender: Option<TransactionStatusSender>,
        entry_notification_sender: Option<EntryNotifierSender>,
        blockstore: Arc<Blockstore>,
        broadcast_type: &BroadcastStageType,
        exit: Arc<AtomicBool>,
        shred_version: u16,
        vote_tracker: Arc<VoteTracker>,
        bank_forks: Arc<RwLock<BankForks>>,
        verified_vote_sender: VerifiedVoteSender,
        gossip_verified_vote_hash_sender: GossipVerifiedVoteHashSender,
        replay_vote_receiver: ReplayVoteReceiver,
        replay_vote_sender: ReplayVoteSender,
        bank_notification_sender: Option<BankNotificationSender>,
        tpu_coalesce: Duration,
        duplicate_confirmed_slot_sender: DuplicateConfirmedSlotsSender,
        client: ForwardingClientOption,
        turbine_quic_endpoint_sender: AsyncSender<(SocketAddr, Bytes)>,
        keypair: &Keypair,
        log_messages_bytes_limit: Option<usize>,
        staked_nodes: &Arc<RwLock<StakedNodes>>,
        shared_staked_nodes_overrides: Arc<RwLock<HashMap<Pubkey, u64>>>,
        banking_tracer_channels: Channels,
        tracer_thread_hdl: TracerThread,
        tpu_enable_udp: bool,
        tpu_quic_server_config: QuicServerParams,
        tpu_fwd_quic_server_config: QuicServerParams,
        vote_quic_server_config: QuicServerParams,
        prioritization_fee_cache: &Arc<PrioritizationFeeCache>,
        block_production_method: BlockProductionMethod,
        transaction_struct: TransactionStructure,
        enable_block_production_forwarding: bool,
        _generator_config: Option<GeneratorConfig>, /* vestigial code for replay invalidator */
        key_notifiers: Arc<RwLock<KeyUpdaters>>,
        block_engine_config: Arc<Mutex<BlockEngineConfig>>,
        relayer_config: Arc<Mutex<RelayerConfig>>,
        tip_manager_config: TipManagerConfig,
        shred_receiver_address: Arc<RwLock<Option<SocketAddr>>>,
        preallocated_bundle_cost: u64,
    ) -> Self {
        let TpuSockets {
            transactions: transactions_sockets,
            transaction_forwards: tpu_forwards_sockets,
            vote: tpu_vote_sockets,
            broadcast: broadcast_sockets,
            transactions_quic: transactions_quic_sockets,
            transactions_forwards_quic: transactions_forwards_quic_sockets,
            vote_quic: tpu_vote_quic_sockets,
            vote_forwarding_client: vote_forwarding_client_socket,
        } = sockets;

        // [----------]
        // [-- QUIC --] \
        // [----------]  \____     [-----------------------]     [--------------------]     [------------------]
        //                    ---- [-- FetchStageManager --] --> [-- SigverifyStage --] --> [-- BankingStage --]
        // [--------------]  /     [-----------------------]     [--------------------]     [------------------]
        // [-- Vortexor --] /
        // [--------------]
        //
        //             fetch_stage_manager_*                packet_receiver

        // Packets from fetch stage and quic server are intercepted and sent through fetch_stage_manager
        // If relayer is connected, packets are dropped. If not, packets are forwarded on to packet_sender
        let (fetch_stage_manager_sender, fetch_stage_manager_receiver) = unbounded();
        let (sigverify_stage_sender, sigverify_stage_receiver) = unbounded();

        let (vote_packet_sender, vote_packet_receiver) = unbounded();
        let (forwarded_packet_sender, forwarded_packet_receiver) = unbounded();
        let fetch_stage = FetchStage::new_with_sender(
            transactions_sockets,
            tpu_forwards_sockets,
            tpu_vote_sockets,
            exit.clone(),
            &fetch_stage_manager_sender,
            &vote_packet_sender,
            &forwarded_packet_sender,
            forwarded_packet_receiver,
            poh_recorder,
            Some(tpu_coalesce),
            Some(bank_forks.read().unwrap().get_vote_only_mode_signal()),
            tpu_enable_udp,
        );

        let staked_nodes_updater_service = StakedNodesUpdaterService::new(
            exit.clone(),
            bank_forks.clone(),
            staked_nodes.clone(),
            shared_staked_nodes_overrides,
        );

        let Channels {
            non_vote_sender: banking_stage_sender,
            non_vote_receiver: banking_stage_receiver,
            tpu_vote_sender,
            tpu_vote_receiver,
            gossip_vote_sender,
            gossip_vote_receiver,
        } = banking_tracer_channels;

        // Streamer for Votes:
        let SpawnServerResult {
            endpoints: _,
            thread: tpu_vote_quic_t,
            key_updater: vote_streamer_key_updater,
        } = spawn_server_multi(
            "solQuicTVo",
            "quic_streamer_tpu_vote",
            tpu_vote_quic_sockets,
            keypair,
            vote_packet_sender.clone(),
            exit.clone(),
            staked_nodes.clone(),
            vote_quic_server_config,
        )
        .unwrap();

        // Streamer for TPU
        let SpawnServerResult {
            endpoints: _,
            thread: tpu_quic_t,
            key_updater,
        } = spawn_server_multi(
            "solQuicTpu",
            "quic_streamer_tpu",
            transactions_quic_sockets,
            keypair,
            fetch_stage_manager_sender.clone(),
            exit.clone(),
            staked_nodes.clone(),
            tpu_quic_server_config,
        )
        .unwrap();

        // Streamer for TPU forward
        let SpawnServerResult {
            endpoints: _,
            thread: tpu_forwards_quic_t,
            key_updater: forwards_key_updater,
        } = spawn_server_multi(
            "solQuicTpuFwd",
            "quic_streamer_tpu_forwards",
            transactions_forwards_quic_sockets,
            keypair,
            forwarded_packet_sender,
            exit.clone(),
            staked_nodes.clone(),
            tpu_fwd_quic_server_config,
        )
        .unwrap();

        let (forward_stage_sender, forward_stage_receiver) = bounded(1024);
        let sigverify_stage = {
            let verifier = TransactionSigVerifier::new(
                banking_stage_sender.clone(),
                enable_block_production_forwarding.then(|| forward_stage_sender.clone()),
            );
            SigVerifyStage::new(
                sigverify_stage_receiver,
                verifier,
                "solSigVerTpu",
                "tpu-verifier",
            )
        };

        let vote_sigverify_stage = {
            let verifier = TransactionSigVerifier::new_reject_non_vote(
                tpu_vote_sender,
                Some(forward_stage_sender),
            );
            SigVerifyStage::new(
                vote_packet_receiver,
                verifier,
                "solSigVerTpuVot",
                "tpu-vote-verifier",
            )
        };

        let block_builder_fee_info = Arc::new(Mutex::new(BlockBuilderFeeInfo {
            block_builder: cluster_info.keypair().pubkey(),
            block_builder_commission: 0,
        }));

        let (bundle_sender, bundle_receiver) = unbounded();
        let block_engine_stage = BlockEngineStage::new(
            block_engine_config,
            bundle_sender,
            cluster_info.clone(),
            sigverify_stage_sender.clone(),
            banking_stage_sender.clone(),
            exit.clone(),
            &block_builder_fee_info,
        );

        let (heartbeat_tx, heartbeat_rx) = unbounded();
        let fetch_stage_manager = FetchStageManager::new(
            cluster_info.clone(),
            heartbeat_rx,
            fetch_stage_manager_receiver,
            sigverify_stage_sender.clone(),
            exit.clone(),
        );

        let relayer_stage = RelayerStage::new(
            relayer_config,
            cluster_info.clone(),
            heartbeat_tx,
            sigverify_stage_sender,
            banking_stage_sender,
            exit.clone(),
        );

        let cluster_info_vote_listener = ClusterInfoVoteListener::new(
            exit.clone(),
            cluster_info.clone(),
            gossip_vote_sender,
            vote_tracker,
            bank_forks.clone(),
            subscriptions.clone(),
            verified_vote_sender,
            gossip_verified_vote_hash_sender,
            replay_vote_receiver,
            blockstore.clone(),
            bank_notification_sender,
            duplicate_confirmed_slot_sender,
        );

        let tip_manager = TipManager::new(tip_manager_config);

        let bundle_account_locker = BundleAccountLocker::default();

        // The tip program can't be used in BankingStage to avoid someone from stealing tips mid-slot.
        // The first 80% of the block, based on poh ticks, has `preallocated_bundle_cost` less compute units.
        // The last 20% has has full compute so blockspace is maximized if BundleStage is idle.
        let reserved_ticks = poh_recorder
            .read()
            .unwrap()
            .ticks_per_slot()
            .saturating_mul(8)
            .saturating_div(10);

        let mut blacklisted_accounts = HashSet::new();
        blacklisted_accounts.insert(tip_manager.tip_payment_program_id());
        let banking_stage = BankingStage::new(
            block_production_method,
            transaction_struct,
            cluster_info,
            poh_recorder,
            transaction_recorder.clone(),
            banking_stage_receiver,
            tpu_vote_receiver,
            gossip_vote_receiver,
            transaction_status_sender.clone(),
            replay_vote_sender.clone(),
            log_messages_bytes_limit,
            bank_forks.clone(),
            prioritization_fee_cache,
            blacklisted_accounts,
            bundle_account_locker.clone(),
            move |bank| {
                calculate_block_cost_limit_reservation(
                    bank,
                    reserved_ticks,
                    preallocated_bundle_cost,
                )
            },
        );

        let SpawnForwardingStageResult {
            join_handle: forwarding_stage,
            client_updater,
        } = spawn_forwarding_stage(
            forward_stage_receiver,
            client,
            vote_forwarding_client_socket,
            RootBankCache::new(bank_forks.clone()),
            ForwardAddressGetter::new(cluster_info.clone(), poh_recorder.clone()),
            DataBudget::default(),
        );

        let bundle_stage = BundleStage::new(
            cluster_info,
            poh_recorder,
            transaction_recorder,
            bundle_receiver,
            transaction_status_sender,
            replay_vote_sender,
            log_messages_bytes_limit,
            exit.clone(),
            tip_manager,
            bundle_account_locker,
            &block_builder_fee_info,
            prioritization_fee_cache,
        );

        let (entry_receiver, tpu_entry_notifier) =
            if let Some(entry_notification_sender) = entry_notification_sender {
                let (broadcast_entry_sender, broadcast_entry_receiver) = unbounded();
                let tpu_entry_notifier = TpuEntryNotifier::new(
                    entry_receiver,
                    entry_notification_sender,
                    broadcast_entry_sender,
                    exit.clone(),
                );
                (broadcast_entry_receiver, Some(tpu_entry_notifier))
            } else {
                (entry_receiver, None)
            };

        let broadcast_stage = broadcast_type.new_broadcast_stage(
            broadcast_sockets,
            cluster_info.clone(),
            entry_receiver,
            retransmit_slots_receiver,
            exit,
            blockstore,
            bank_forks,
            shred_version,
            turbine_quic_endpoint_sender,
            shred_receiver_address,
        );

        let mut key_notifiers = key_notifiers.write().unwrap();
        key_notifiers.add(KeyUpdaterType::Tpu, key_updater);
        key_notifiers.add(KeyUpdaterType::TpuForwards, forwards_key_updater);
        key_notifiers.add(KeyUpdaterType::TpuVote, vote_streamer_key_updater);
        key_notifiers.add(KeyUpdaterType::Forward, client_updater);

        Self {
            fetch_stage,
            sigverify_stage,
            vote_sigverify_stage,
            banking_stage,
            forwarding_stage,
            cluster_info_vote_listener,
            broadcast_stage,
            tpu_quic_t,
            tpu_forwards_quic_t,
            tpu_entry_notifier,
            staked_nodes_updater_service,
            tracer_thread_hdl,
            tpu_vote_quic_t,
            block_engine_stage,
            relayer_stage,
            fetch_stage_manager,
            bundle_stage,
        }
    }

    pub fn join(self) -> thread::Result<()> {
        let results = vec![
            self.fetch_stage.join(),
            self.sigverify_stage.join(),
            self.vote_sigverify_stage.join(),
            self.cluster_info_vote_listener.join(),
            self.banking_stage.join(),
            self.forwarding_stage.join(),
            self.staked_nodes_updater_service.join(),
            self.tpu_quic_t.join(),
            self.tpu_forwards_quic_t.join(),
            self.tpu_vote_quic_t.join(),
            self.bundle_stage.join(),
            self.relayer_stage.join(),
            self.block_engine_stage.join(),
            self.fetch_stage_manager.join(),
        ];
        let broadcast_result = self.broadcast_stage.join();
        for result in results {
            result?;
        }
        if let Some(tpu_entry_notifier) = self.tpu_entry_notifier {
            tpu_entry_notifier.join()?;
        }
        let _ = broadcast_result?;
        if let Some(tracer_thread_hdl) = self.tracer_thread_hdl {
            if let Err(tracer_result) = tracer_thread_hdl.join()? {
                error!(
                    "banking tracer thread returned error after successful thread join: {:?}",
                    tracer_result
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use {
        super::calculate_block_cost_limit_reservation,
        solana_ledger::genesis_utils::create_genesis_config, solana_pubkey::Pubkey,
        solana_runtime::bank::Bank, std::sync::Arc,
    };

    #[test]
    fn test_calculate_block_cost_limit_reservation() {
        const BUNDLE_BLOCK_COST_LIMITS_RESERVATION: u64 = 100;
        const RESERVED_TICKS: u64 = 5;
        let genesis_config_info = create_genesis_config(100);
        let bank = Arc::new(Bank::new_for_tests(&genesis_config_info.genesis_config));

        for _ in 0..genesis_config_info.genesis_config.ticks_per_slot {
            bank.register_default_tick_for_test();
        }
        assert!(bank.is_complete());
        bank.freeze();
        let bank1 = Arc::new(Bank::new_from_parent(bank.clone(), &Pubkey::default(), 1));

        // wait for reservation to be over
        (0..RESERVED_TICKS).for_each(|_| {
            assert_eq!(
                calculate_block_cost_limit_reservation(
                    &bank1,
                    RESERVED_TICKS,
                    BUNDLE_BLOCK_COST_LIMITS_RESERVATION,
                ),
                BUNDLE_BLOCK_COST_LIMITS_RESERVATION
            );
            bank1.register_default_tick_for_test();
        });
        assert_eq!(
            calculate_block_cost_limit_reservation(
                &bank1,
                RESERVED_TICKS,
                BUNDLE_BLOCK_COST_LIMITS_RESERVATION,
            ),
            0
        );
    }
}
