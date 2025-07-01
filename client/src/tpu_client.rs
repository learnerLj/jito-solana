//! # TPU Client Module
//!
//! This module provides direct communication with Solana's Transaction Processing Unit (TPU)
//! for high-performance transaction submission. The TPU client bypasses the RPC layer
//! and sends transactions directly to the validator's TPU port for reduced latency.
//!
//! ## Key Features
//!
//! - **Direct TPU Communication**: Bypasses RPC bottlenecks for faster transaction submission
//! - **Leader Tracking**: Automatically determines current and upcoming leaders
//! - **Fanout Strategy**: Sends transactions to multiple upcoming leaders for better inclusion odds
//! - **Protocol Flexibility**: Supports both UDP and QUIC transport protocols
//! - **Batch Processing**: Efficient handling of transaction batches
//!
//! ## Architecture
//!
//! The TPU client operates by:
//! 1. Monitoring the validator network to identify current and upcoming leaders
//! 2. Sending transactions directly to leader TPU ports
//! 3. Using a fanout strategy to increase transaction inclusion probability
//! 4. Providing both blocking and async interfaces for different use cases
//!
//! ## Performance Considerations
//!
//! - TPU communication reduces latency compared to RPC submission
//! - Fanout strategy improves transaction inclusion rates during leader transitions
//! - QUIC protocol provides better performance than UDP for high-volume scenarios
//! - Batch operations significantly improve throughput for multiple transactions

use {
    crate::connection_cache::ConnectionCache,
    solana_connection_cache::connection_cache::{
        ConnectionCache as BackendConnectionCache, ConnectionManager, ConnectionPool,
        NewConnectionConfig,
    },
    solana_message::Message,
    solana_quic_client::{QuicConfig, QuicConnectionManager, QuicPool},
    solana_rpc_client::rpc_client::RpcClient,
    solana_signer::signers::Signers,
    solana_tpu_client::tpu_client::{Result, TpuClient as BackendTpuClient},
    solana_transaction::Transaction,
    solana_transaction_error::{TransactionError, TransportResult},
    solana_udp_client::{UdpConfig, UdpConnectionManager, UdpPool},
    std::sync::Arc,
};
pub use {
    crate::nonblocking::tpu_client::TpuSenderError,
    solana_tpu_client::tpu_client::{TpuClientConfig, DEFAULT_FANOUT_SLOTS, MAX_FANOUT_SLOTS},
};

/// A wrapper enum for different TPU client protocol implementations.
///
/// This enum allows the system to abstract over different transport protocols
/// while maintaining type safety and protocol-specific optimizations.
pub enum TpuClientWrapper {
    /// QUIC-based TPU client for high-performance, encrypted communication
    Quic(BackendTpuClient<QuicPool, QuicConnectionManager, QuicConfig>),
    /// UDP-based TPU client for simple, stateless communication
    Udp(BackendTpuClient<UdpPool, UdpConnectionManager, UdpConfig>),
}

/// High-performance client for direct TPU communication.
///
/// This client sends transactions directly to validator TPU ports, bypassing the RPC layer
/// for reduced latency. It automatically tracks leader rotation and uses a fanout strategy
/// to maximize transaction inclusion probability.
///
/// ## Generic Parameters
///
/// * `P` - ConnectionPool: Manages connection pooling for the transport protocol
/// * `M` - ConnectionManager: Handles connection lifecycle and configuration  
/// * `C` - NewConnectionConfig: Protocol-specific connection configuration
///
/// ## Usage
///
/// ```rust
/// // Create a QUIC-based TPU client
/// let client = TpuClient::new(rpc_client, websocket_url, config)?;
///
/// // Send a single transaction
/// let success = client.send_transaction(&transaction);
///
/// // Send a batch of transactions
/// client.try_send_transaction_batch(&transactions)?;
/// ```
///
/// ## Performance Notes
///
/// This is a thin wrapper over the BackendTpuClient. For maximum efficiency in
/// performance-critical applications, consider using BackendTpuClient directly.
pub struct TpuClient<
    P, // ConnectionPool
    M, // ConnectionManager
    C, // NewConnectionConfig
> {
    tpu_client: BackendTpuClient<P, M, C>,
}

impl<P, M, C> TpuClient<P, M, C>
where
    P: ConnectionPool<NewConnectionConfig = C>,
    M: ConnectionManager<ConnectionPool = P, NewConnectionConfig = C>,
    C: NewConnectionConfig,
{
    /// Sends a transaction to current and upcoming leader TPUs.
    ///
    /// This method serializes the transaction and sends it to multiple TPU endpoints
    /// based on the configured fanout size. The fanout strategy increases the
    /// probability of transaction inclusion during leader transitions.
    ///
    /// # Arguments
    /// * `transaction` - The transaction to send
    ///
    /// # Returns
    /// `true` if the transaction was successfully sent to at least one TPU,
    /// `false` if all send attempts failed
    pub fn send_transaction(&self, transaction: &Transaction) -> bool {
        self.tpu_client.send_transaction(transaction)
    }

    /// Sends a pre-serialized transaction to TPU endpoints.
    ///
    /// This method is optimized for scenarios where transactions are already
    /// serialized, avoiding redundant serialization overhead.
    ///
    /// # Arguments
    /// * `wire_transaction` - Pre-serialized transaction bytes
    ///
    /// # Returns
    /// `true` if successfully sent to at least one TPU, `false` otherwise
    pub fn send_wire_transaction(&self, wire_transaction: Vec<u8>) -> bool {
        self.tpu_client.send_wire_transaction(wire_transaction)
    }

    /// Attempts to send a transaction with error reporting.
    ///
    /// Similar to send_transaction but returns detailed error information
    /// if all fanout attempts fail. Useful for error handling and debugging.
    ///
    /// # Arguments
    /// * `transaction` - The transaction to send
    ///
    /// # Returns
    /// `Ok(())` if successfully sent to at least one TPU,
    /// `Err(TransportError)` containing the last error if all attempts failed
    pub fn try_send_transaction(&self, transaction: &Transaction) -> TransportResult<()> {
        self.tpu_client.try_send_transaction(transaction)
    }

    /// Attempts to send a batch of transactions efficiently.
    ///
    /// This method optimizes network utilization by sending multiple transactions
    /// in batch operations. It's significantly more efficient than individual
    /// send operations for bulk transaction submission.
    ///
    /// # Arguments
    /// * `transactions` - Slice of transactions to send as a batch
    ///
    /// # Returns
    /// `Ok(())` if the batch was successfully sent to at least one TPU,
    /// `Err(TransportError)` if all batch send attempts failed
    pub fn try_send_transaction_batch(&self, transactions: &[Transaction]) -> TransportResult<()> {
        self.tpu_client.try_send_transaction_batch(transactions)
    }

    /// Attempts to send a pre-serialized transaction with error reporting.
    ///
    /// Combines the efficiency of wire transactions with comprehensive error
    /// reporting for robust error handling in production systems.
    ///
    /// # Arguments
    /// * `wire_transaction` - Pre-serialized transaction bytes
    ///
    /// # Returns
    /// `Ok(())` if successfully sent, `Err(TransportError)` with details if failed
    pub fn try_send_wire_transaction(&self, wire_transaction: Vec<u8>) -> TransportResult<()> {
        self.tpu_client.try_send_wire_transaction(wire_transaction)
    }
}

/// QUIC-specific implementation for TPU client.
///
/// This implementation is optimized for QUIC protocol connections, providing
/// enhanced performance through multiplexing, built-in encryption, and
/// connection efficiency.
impl TpuClient<QuicPool, QuicConnectionManager, QuicConfig> {
    /// Creates a new QUIC-based TPU client.
    ///
    /// This constructor automatically configures a QUIC connection cache and
    /// sets up the client for high-performance TPU communication.
    ///
    /// # Arguments
    /// * `rpc_client` - Shared RPC client for blockchain queries and leader tracking
    /// * `websocket_url` - WebSocket endpoint for real-time validator updates
    /// * `config` - TPU client configuration (fanout size, timeouts, etc.)
    ///
    /// # Returns
    /// A configured TPU client ready for transaction submission
    ///
    /// # Errors
    /// Returns error if QUIC connection setup fails or websocket connection fails
    pub fn new(
        rpc_client: Arc<RpcClient>,
        websocket_url: &str,
        config: TpuClientConfig,
    ) -> Result<Self> {
        let connection_cache = match ConnectionCache::new("connection_cache_tpu_client") {
            ConnectionCache::Quic(cache) => cache,
            ConnectionCache::Udp(_) => {
                return Err(TpuSenderError::Custom(String::from(
                    "Invalid default connection cache",
                )))
            }
        };
        Self::new_with_connection_cache(rpc_client, websocket_url, config, connection_cache)
    }
}

/// Generic implementation for all TPU client protocol types.
///
/// This implementation provides protocol-agnostic functionality that works
/// with any connection pool, manager, and configuration combination.
impl<P, M, C> TpuClient<P, M, C>
where
    P: ConnectionPool<NewConnectionConfig = C>,
    M: ConnectionManager<ConnectionPool = P, NewConnectionConfig = C>,
    C: NewConnectionConfig,
{
    /// Creates a TPU client with a custom connection cache.
    ///
    /// This constructor allows using pre-configured connection caches,
    /// enabling fine-grained control over connection parameters and
    /// sharing connection pools across multiple client instances.
    ///
    /// # Arguments
    /// * `rpc_client` - Shared RPC client for validator communication
    /// * `websocket_url` - WebSocket endpoint for leader updates
    /// * `config` - TPU client configuration parameters
    /// * `connection_cache` - Pre-configured connection cache
    ///
    /// # Returns
    /// A TPU client using the provided connection cache
    ///
    /// # Errors
    /// Returns error if client initialization or websocket setup fails
    pub fn new_with_connection_cache(
        rpc_client: Arc<RpcClient>,
        websocket_url: &str,
        config: TpuClientConfig,
        connection_cache: Arc<BackendConnectionCache<P, M, C>>,
    ) -> Result<Self> {
        Ok(Self {
            tpu_client: BackendTpuClient::new_with_connection_cache(
                rpc_client,
                websocket_url,
                config,
                connection_cache,
            )?,
        })
    }

    /// Sends and confirms multiple messages with visual progress indication.
    ///
    /// This method provides a high-level interface for batch transaction processing
    /// with built-in progress visualization. It handles message signing, submission,
    /// and confirmation tracking automatically.
    ///
    /// # Arguments
    /// * `messages` - Array of messages to process as transactions
    /// * `signers` - Collection of keypairs for transaction signing
    ///
    /// # Returns
    /// Vector of optional transaction errors (None = success, Some(err) = failure)
    ///
    /// # Features
    /// - Visual progress spinner for user feedback
    /// - Automatic retry logic for failed transactions
    /// - Parallel processing for improved throughput
    /// - Comprehensive error reporting per transaction
    pub fn send_and_confirm_messages_with_spinner<T: Signers + ?Sized>(
        &self,
        messages: &[Message],
        signers: &T,
    ) -> Result<Vec<Option<TransactionError>>> {
        self.tpu_client
            .send_and_confirm_messages_with_spinner(messages, signers)
    }

    /// Returns a reference to the underlying RPC client.
    ///
    /// Provides access to the RPC client for direct blockchain queries
    /// that complement TPU transaction submission capabilities.
    ///
    /// # Returns
    /// Reference to the shared RPC client instance
    pub fn rpc_client(&self) -> &RpcClient {
        self.tpu_client.rpc_client()
    }
}
