//! # Async TPU Client Module
//!
//! This module provides asynchronous TPU (Transaction Processing Unit) client functionality
//! for high-performance, non-blocking transaction submission to Solana validators.
//! It's optimized for applications requiring high throughput and concurrent transaction processing.
//!
//! ## Key Features
//!
//! - **Async/Await Interface**: Full async support for non-blocking operations
//! - **High Concurrency**: Handle thousands of concurrent transaction submissions
//! - **Leader Tracking**: Automatic tracking of current and upcoming leader validators
//! - **Protocol Support**: Both UDP and QUIC transport protocols
//! - **Batch Processing**: Efficient batch transaction submission
//!
//! ## Performance Benefits
//!
//! - **Direct TPU Access**: Bypasses RPC layer for reduced latency
//! - **Fanout Strategy**: Sends to multiple leaders for better inclusion odds
//! - **Connection Pooling**: Efficient connection reuse across requests
//! - **Async I/O**: Non-blocking network operations for maximum throughput
//!
//! ## Usage Example
//!
//! ```rust
//! use solana_client::nonblocking::tpu_client::TpuClient;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let rpc_client = Arc::new(RpcClient::new("https://api.mainnet-beta.solana.com".to_string()));
//!     let tpu_client = TpuClient::new(
//!         "my_app",
//!         rpc_client,
//!         "wss://api.mainnet-beta.solana.com/",
//!         TpuClientConfig::default(),
//!     ).await?;
//!
//!     let success = tpu_client.send_transaction(&transaction).await;
//!     Ok(())
//! }
//! ```

// Re-export core TPU types for convenience
pub use solana_tpu_client::nonblocking::tpu_client::{LeaderTpuService, TpuSenderError};
use {
    crate::{connection_cache::ConnectionCache, tpu_client::TpuClientConfig},
    solana_connection_cache::connection_cache::{
        ConnectionCache as BackendConnectionCache, ConnectionManager, ConnectionPool,
        NewConnectionConfig,
    },
    solana_message::Message,
    solana_quic_client::{QuicConfig, QuicConnectionManager, QuicPool},
    solana_rpc_client::nonblocking::rpc_client::RpcClient,
    solana_signer::signers::Signers,
    solana_tpu_client::nonblocking::tpu_client::{Result, TpuClient as BackendTpuClient},
    solana_transaction::Transaction,
    solana_transaction_error::{TransactionError, TransportResult},
    std::sync::Arc,
};

/// Async client for direct TPU communication with validators.
///
/// This client provides high-performance, asynchronous transaction submission
/// directly to validator TPU ports. It automatically tracks leader rotation
/// and uses intelligent fanout strategies to maximize transaction inclusion rates.
///
/// ## Generic Parameters
///
/// * `P` - ConnectionPool: Manages connection pooling for network efficiency
/// * `M` - ConnectionManager: Handles connection lifecycle and configuration
/// * `C` - NewConnectionConfig: Protocol-specific connection settings
///
/// ## Architecture
///
/// The client operates by:
/// 1. Monitoring validator network to identify current/upcoming leaders
/// 2. Maintaining persistent connections to leader TPU endpoints
/// 3. Using fanout submission to multiple leaders during transitions
/// 4. Providing async interfaces for non-blocking transaction processing
///
/// ## Performance Characteristics
///
/// - **Low Latency**: Direct TPU communication eliminates RPC overhead
/// - **High Throughput**: Async processing enables massive concurrency
/// - **Reliability**: Fanout strategy improves transaction inclusion rates
/// - **Efficiency**: Connection pooling minimizes network overhead
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
    /// Asynchronously sends a transaction to current and upcoming leader TPUs.
    ///
    /// This method serializes the transaction and distributes it to multiple TPU
    /// endpoints based on the configured fanout size. The async nature allows
    /// for efficient concurrent processing without blocking the caller.
    ///
    /// # Arguments
    /// * `transaction` - The transaction to submit to the network
    ///
    /// # Returns
    /// `true` if the transaction was successfully sent to at least one TPU endpoint,
    /// `false` if all submission attempts failed
    ///
    /// # Performance Notes
    /// - Uses connection pooling for efficient network utilization
    /// - Implements intelligent retry logic for transient failures
    /// - Distributes to multiple leaders to handle leader transitions gracefully
    pub async fn send_transaction(&self, transaction: &Transaction) -> bool {
        self.tpu_client.send_transaction(transaction).await
    }

    /// Asynchronously sends a pre-serialized transaction to TPU endpoints.
    ///
    /// This method is optimized for scenarios where transactions are already
    /// serialized, eliminating redundant serialization overhead. Particularly
    /// useful for batch processing and high-frequency submission scenarios.
    ///
    /// # Arguments
    /// * `wire_transaction` - Pre-serialized transaction bytes ready for network transmission
    ///
    /// # Returns
    /// `true` if successfully transmitted to at least one TPU, `false` otherwise
    ///
    /// # Performance Advantages
    /// - Avoids serialization overhead for pre-processed transactions
    /// - Optimal for batch operations where serialization can be done once
    /// - Reduces CPU usage in high-throughput scenarios
    pub async fn send_wire_transaction(&self, wire_transaction: Vec<u8>) -> bool {
        self.tpu_client
            .send_wire_transaction(wire_transaction)
            .await
    }

    /// Attempts to send a transaction with comprehensive error reporting.
    ///
    /// This method provides the same functionality as `send_transaction` but
    /// returns detailed error information when all fanout attempts fail.
    /// Essential for applications requiring robust error handling and debugging.
    ///
    /// # Arguments
    /// * `transaction` - The transaction to submit to the network
    ///
    /// # Returns
    /// * `Ok(())` - Transaction successfully sent to at least one TPU
    /// * `Err(TransportError)` - All submission attempts failed with error details
    ///
    /// # Error Handling
    /// The returned error contains information about the last failure encountered,
    /// which can be used for debugging network issues or implementing custom retry logic.
    pub async fn try_send_transaction(&self, transaction: &Transaction) -> TransportResult<()> {
        self.tpu_client.try_send_transaction(transaction).await
    }

    /// Attempts to send a pre-serialized transaction with error reporting.
    ///
    /// Combines the efficiency of wire transaction submission with comprehensive
    /// error reporting. Ideal for production systems requiring both performance
    /// and reliable error handling.
    ///
    /// # Arguments
    /// * `wire_transaction` - Pre-serialized transaction bytes
    ///
    /// # Returns
    /// * `Ok(())` - Successfully sent to at least one TPU endpoint
    /// * `Err(TransportError)` - All attempts failed with detailed error information
    ///
    /// # Use Cases
    /// - Production transaction submission with error logging
    /// - Implementing custom retry strategies based on error types
    /// - Monitoring and alerting on submission failures
    pub async fn try_send_wire_transaction(
        &self,
        wire_transaction: Vec<u8>,
    ) -> TransportResult<()> {
        self.tpu_client
            .try_send_wire_transaction(wire_transaction)
            .await
    }

    /// Asynchronously sends a batch of pre-serialized transactions.
    ///
    /// This method optimizes network utilization by submitting multiple transactions
    /// in batch operations. It's significantly more efficient than individual submissions
    /// for bulk transaction processing scenarios.
    ///
    /// # Arguments
    /// * `wire_transactions` - Vector of pre-serialized transaction bytes
    ///
    /// # Returns
    /// * `Ok(())` - Batch successfully sent to at least one TPU endpoint
    /// * `Err(TransportError)` - All batch submission attempts failed
    ///
    /// # Performance Benefits
    /// - Reduces network round trips through batching
    /// - Optimizes connection utilization across multiple transactions
    /// - Ideal for bulk operations like airdrops or batch payments
    /// - Minimizes per-transaction overhead in high-volume scenarios
    ///
    /// # Batch Size Considerations
    /// Large batches may exceed network MTU limits. The implementation
    /// automatically handles optimal batch sizing for the underlying transport.
    pub async fn try_send_wire_transaction_batch(
        &self,
        wire_transactions: Vec<Vec<u8>>,
    ) -> TransportResult<()> {
        self.tpu_client
            .try_send_wire_transaction_batch(wire_transactions)
            .await
    }
}

/// QUIC-specific async TPU client implementation.
///
/// This implementation is optimized for QUIC protocol connections, providing
/// superior performance through connection multiplexing, built-in encryption,
/// and reduced connection overhead.
impl TpuClient<QuicPool, QuicConnectionManager, QuicConfig> {
    /// Creates a new async QUIC-based TPU client.
    ///
    /// This constructor initializes a high-performance TPU client using QUIC protocol
    /// for optimal transaction submission performance. The client automatically
    /// manages connections and handles leader rotation.
    ///
    /// # Arguments
    /// * `name` - Static string identifier for this client instance (used for metrics)
    /// * `rpc_client` - Shared async RPC client for validator queries and leader tracking
    /// * `websocket_url` - WebSocket endpoint URL for real-time validator updates
    /// * `config` - TPU client configuration (fanout size, timeouts, retry settings)
    ///
    /// # Returns
    /// An async TPU client ready for high-performance transaction submission
    ///
    /// # Errors
    /// * Connection setup failures (network, authentication, protocol negotiation)
    /// * WebSocket subscription failures for leader updates
    /// * Invalid configuration parameters
    ///
    /// # Example
    /// ```rust
    /// let client = TpuClient::new(
    ///     "my_trading_bot",
    ///     rpc_client,
    ///     "wss://api.mainnet-beta.solana.com/",
    ///     TpuClientConfig::default(),
    /// ).await?;
    /// ```
    pub async fn new(
        name: &'static str,
        rpc_client: Arc<RpcClient>,
        websocket_url: &str,
        config: TpuClientConfig,
    ) -> Result<Self> {
        let connection_cache = match ConnectionCache::new(name) {
            ConnectionCache::Quic(cache) => cache,
            ConnectionCache::Udp(_) => {
                return Err(TpuSenderError::Custom(String::from(
                    "Invalid default connection cache",
                )))
            }
        };
        Self::new_with_connection_cache(rpc_client, websocket_url, config, connection_cache).await
    }
}

/// Generic async implementation for all supported transport protocols.
///
/// This implementation provides protocol-agnostic functionality that works
/// with any combination of connection pool, manager, and configuration types.
/// It enables flexibility in choosing transport protocols while maintaining
/// consistent async interfaces.
impl<P, M, C> TpuClient<P, M, C>
where
    P: ConnectionPool<NewConnectionConfig = C>,
    M: ConnectionManager<ConnectionPool = P, NewConnectionConfig = C>,
    C: NewConnectionConfig,
{
    /// Creates an async TPU client with a custom connection cache.
    ///
    /// This constructor enables fine-grained control over connection management
    /// by accepting a pre-configured connection cache. Useful for sharing
    /// connection pools across multiple client instances or implementing
    /// custom connection strategies.
    ///
    /// # Arguments
    /// * `rpc_client` - Shared async RPC client for blockchain communication
    /// * `websocket_url` - WebSocket endpoint for real-time leader notifications
    /// * `config` - TPU client configuration parameters
    /// * `connection_cache` - Pre-configured connection cache with custom settings
    ///
    /// # Returns
    /// An async TPU client using the provided connection infrastructure
    ///
    /// # Errors
    /// * Client initialization failures
    /// * WebSocket connection or subscription failures
    /// * Invalid connection cache configuration
    ///
    /// # Advanced Usage
    /// This method is ideal for:
    /// - Sharing connection pools across multiple clients
    /// - Implementing custom connection retry strategies
    /// - Fine-tuning protocol-specific parameters
    /// - Resource optimization in multi-client applications
    pub async fn new_with_connection_cache(
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
            )
            .await?,
        })
    }

    /// Asynchronously processes and confirms multiple messages with progress indication.
    ///
    /// This high-level method provides a complete transaction processing pipeline
    /// with visual progress feedback. It handles message signing, parallel submission,
    /// confirmation tracking, and comprehensive error reporting.
    ///
    /// # Arguments
    /// * `messages` - Array of transaction messages to process
    /// * `signers` - Collection of keypairs for transaction signing
    ///
    /// # Returns
    /// Vector of optional transaction errors where:
    /// - `None` indicates successful transaction confirmation
    /// - `Some(TransactionError)` indicates the specific failure reason
    ///
    /// # Features
    /// - **Visual Progress**: Interactive spinner showing processing status
    /// - **Parallel Processing**: Concurrent submission and confirmation tracking
    /// - **Automatic Retries**: Smart retry logic for transient failures
    /// - **Comprehensive Reporting**: Detailed error information per transaction
    /// - **Graceful Handling**: Continues processing even if some transactions fail
    ///
    /// # Performance Characteristics
    /// - Optimized for batch processing of multiple transactions
    /// - Uses async concurrency for maximum throughput
    /// - Implements efficient confirmation polling strategies
    /// - Provides real-time feedback for long-running operations
    ///
    /// # Example
    /// ```rust
    /// let results = tpu_client.send_and_confirm_messages_with_spinner(
    ///     &messages,
    ///     &keypairs,
    /// ).await?;
    ///
    /// for (i, result) in results.iter().enumerate() {
    ///     match result {
    ///         None => println!("Transaction {} confirmed", i),
    ///         Some(err) => println!("Transaction {} failed: {:?}", i, err),
    ///     }
    /// }
    /// ```
    pub async fn send_and_confirm_messages_with_spinner<T: Signers + ?Sized>(
        &self,
        messages: &[Message],
        signers: &T,
    ) -> Result<Vec<Option<TransactionError>>> {
        self.tpu_client
            .send_and_confirm_messages_with_spinner(messages, signers)
            .await
    }

    /// Returns a reference to the underlying async RPC client.
    ///
    /// Provides access to the shared RPC client for performing additional
    /// blockchain queries that complement TPU transaction submission.
    /// The RPC client can be used for account queries, program calls,
    /// and other read operations.
    ///
    /// # Returns
    /// Reference to the async RPC client instance
    ///
    /// # Usage
    /// ```rust
    /// let rpc = tpu_client.rpc_client();
    /// let balance = rpc.get_balance(&pubkey).await?;
    /// let slot = rpc.get_slot().await?;
    /// ```
    pub fn rpc_client(&self) -> &RpcClient {
        self.tpu_client.rpc_client()
    }

    /// Gracefully shuts down the TPU client and releases resources.
    ///
    /// This method performs a clean shutdown of the client, closing all
    /// active connections, stopping background tasks, and releasing
    /// system resources. Should be called when the client is no longer needed.
    ///
    /// # Behavior
    /// - Closes all active TPU connections
    /// - Stops leader tracking and update subscriptions
    /// - Cancels pending operations gracefully
    /// - Releases connection pool resources
    ///
    /// # Usage
    /// ```rust
    /// let mut tpu_client = TpuClient::new(...).await?;
    /// // ... use the client ...
    /// tpu_client.shutdown().await;
    /// ```
    ///
    /// # Note
    /// After calling shutdown, the client should not be used for further operations.
    /// Create a new client instance if TPU functionality is needed again.
    pub async fn shutdown(&mut self) {
        self.tpu_client.shutdown().await
    }
}
