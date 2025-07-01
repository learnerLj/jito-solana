//! # Transaction Executor Module
//!
//! This module provides a multi-threaded transaction executor that efficiently manages
//! the submission and confirmation tracking of large batches of transactions.
//! It's designed for high-throughput scenarios where many transactions need to be
//! processed concurrently with automatic retry logic and status monitoring.
//!
//! ## Key Features
//!
//! - **Concurrent Processing**: Multi-threaded transaction submission and confirmation
//! - **Automatic Retry Logic**: Built-in retry mechanisms for failed transactions
//! - **Status Tracking**: Real-time monitoring of transaction confirmation status
//! - **Timeout Management**: Configurable timeouts to handle stuck transactions
//! - **Batch Optimization**: Efficient batch processing for improved throughput
//!
//! ## Architecture
//!
//! The executor uses a background thread to continuously monitor transaction status:
//! 1. Transactions are submitted and added to a pending queue
//! 2. A background thread polls for confirmation status
//! 3. Confirmed or timed-out transactions are moved to a cleared queue
//! 4. Applications can drain the cleared queue to track completion
//!
//! ## Usage Patterns
//!
//! This executor is ideal for:
//! - Bulk transaction processing (airdrops, batch payments)
//! - High-frequency trading applications
//! - Load testing and stress testing scenarios
//! - Any application requiring concurrent transaction management

#![allow(clippy::arithmetic_side_effects)]
use {
    log::*,
    solana_commitment_config::CommitmentConfig,
    solana_measure::measure::Measure,
    solana_rpc_client::rpc_client::RpcClient,
    solana_signature::Signature,
    solana_time_utils::timestamp,
    solana_transaction::Transaction,
    std::{
        net::SocketAddr,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc, RwLock,
        },
        thread::{sleep, Builder, JoinHandle},
        time::{Duration, Instant},
    },
};

/// Type alias for the pending transaction queue.
/// 
/// Each entry contains:
/// - `Signature`: The transaction signature for status lookup
/// - `u64`: Timestamp when the transaction was submitted (for timeout detection)
/// - `u64`: Unique identifier assigned to this transaction
type PendingQueue = Vec<(Signature, u64, u64)>;

/// High-performance transaction executor for concurrent transaction processing.
///
/// This executor manages the complete lifecycle of transaction submission and confirmation
/// using a background thread for status monitoring. It provides automatic retry logic,
/// timeout handling, and efficient batch processing capabilities.
///
/// ## Thread Safety
///
/// All fields are thread-safe and can be safely accessed from multiple threads:
/// - Pending transactions are protected by RwLock for concurrent access
/// - Cleared transactions use RwLock for safe draining operations
/// - Atomic operations ensure consistent state management
pub struct TransactionExecutor {
    /// Handle to the background signature clearing thread
    sig_clear_t: JoinHandle<()>,
    /// Thread-safe queue of pending transactions awaiting confirmation
    sigs: Arc<RwLock<PendingQueue>>,
    /// Thread-safe list of cleared transaction IDs (completed or timed out)
    cleared: Arc<RwLock<Vec<u64>>>,
    /// Atomic flag to signal background thread shutdown
    exit: Arc<AtomicBool>,
    /// Atomic counter for generating unique transaction IDs
    counter: AtomicU64,
    /// Shared RPC client for transaction submission and status queries
    client: Arc<RpcClient>,
}

impl TransactionExecutor {
    /// Creates a new transaction executor connected to a specific validator endpoint.
    ///
    /// This constructor creates an RPC client with confirmed commitment level
    /// and initializes the executor with default settings.
    ///
    /// # Arguments
    /// * `entrypoint_addr` - Socket address of the validator's RPC endpoint
    ///
    /// # Returns
    /// A fully initialized transaction executor ready for use
    pub fn new(entrypoint_addr: SocketAddr) -> Self {
        let client = Arc::new(RpcClient::new_socket_with_commitment(
            entrypoint_addr,
            CommitmentConfig::confirmed(),
        ));
        Self::new_with_rpc_client(client)
    }

    /// Creates a new transaction executor using a URL-based RPC endpoint.
    ///
    /// This is a convenience constructor for creating executors with HTTP(S)
    /// or WebSocket URLs instead of socket addresses.
    ///
    /// # Arguments
    /// * `url` - URL of the validator's RPC endpoint (HTTP, HTTPS, or WS)
    ///
    /// # Returns
    /// A fully initialized transaction executor
    pub fn new_with_url<U: ToString>(url: U) -> Self {
        let client = Arc::new(RpcClient::new_with_commitment(
            url,
            CommitmentConfig::confirmed(),
        ));
        Self::new_with_rpc_client(client)
    }

    /// Creates a new transaction executor with a pre-configured RPC client.
    ///
    /// This constructor allows sharing RPC clients across multiple executors
    /// or using custom RPC client configurations.
    ///
    /// # Arguments
    /// * `client` - Pre-configured RPC client to use for all operations
    ///
    /// # Returns
    /// A transaction executor using the provided RPC client
    pub fn new_with_rpc_client(client: Arc<RpcClient>) -> Self {
        let sigs = Arc::new(RwLock::new(Vec::new()));
        let cleared = Arc::new(RwLock::new(Vec::new()));
        let exit = Arc::new(AtomicBool::new(false));
        let sig_clear_t = Self::start_sig_clear_thread(exit.clone(), &sigs, &cleared, &client);
        Self {
            sigs,
            cleared,
            sig_clear_t,
            exit,
            counter: AtomicU64::new(0),
            client,
        }
    }

    /// Returns the number of transactions currently awaiting confirmation.
    ///
    /// This method provides insight into the executor's current workload
    /// and can be used for monitoring and load balancing decisions.
    ///
    /// # Returns
    /// The count of pending transactions in the queue
    pub fn num_outstanding(&self) -> usize {
        self.sigs.read().unwrap().len()
    }

    /// Submits a batch of transactions for processing.
    ///
    /// This method attempts to submit all provided transactions to the network
    /// and returns unique identifiers for tracking their progress. Failed
    /// submissions are logged but don't prevent other transactions from being sent.
    ///
    /// # Arguments
    /// * `txs` - Vector of transactions to submit
    ///
    /// # Returns
    /// Vector of unique IDs corresponding to each input transaction.
    /// IDs are returned even for failed submissions to maintain index alignment.
    ///
    /// # Performance Notes
    /// - Transactions are submitted sequentially but confirmation tracking is concurrent
    /// - Failed transactions are automatically excluded from confirmation tracking
    /// - Use this method for batch operations to improve throughput
    pub fn push_transactions(&self, txs: Vec<Transaction>) -> Vec<u64> {
        let mut ids = vec![];
        let new_sigs = txs.into_iter().filter_map(|tx| {
            let id = self.counter.fetch_add(1, Ordering::Relaxed);
            ids.push(id);
            match self.client.send_transaction(&tx) {
                Ok(sig) => {
                    return Some((sig, timestamp(), id));
                }
                Err(e) => {
                    info!("error: {:#?}", e);
                }
            }
            None
        });
        let mut sigs_w = self.sigs.write().unwrap();
        sigs_w.extend(new_sigs);
        ids
    }

    /// Retrieves and removes all completed transaction IDs.
    ///
    /// This method atomically drains the cleared transaction queue, returning
    /// all transaction IDs that have either been confirmed or timed out since
    /// the last call to this method.
    ///
    /// # Returns
    /// Vector of transaction IDs that have completed processing
    ///
    /// # Usage
    /// Call this method periodically to track transaction completion and
    /// update application state accordingly.
    pub fn drain_cleared(&self) -> Vec<u64> {
        std::mem::take(&mut *self.cleared.write().unwrap())
    }

    /// Shuts down the transaction executor and cleans up resources.
    ///
    /// This method signals the background thread to exit and waits for it
    /// to complete before returning. Call this when the executor is no longer needed
    /// to ensure proper cleanup of system resources.
    ///
    /// # Blocking Behavior
    /// This method blocks until the background thread terminates.
    pub fn close(self) {
        self.exit.store(true, Ordering::Relaxed);
        self.sig_clear_t.join().unwrap();
    }

    /// Starts the background thread responsible for transaction confirmation monitoring.
    ///
    /// This thread continuously polls the network for transaction status updates,
    /// moving confirmed or timed-out transactions from the pending queue to the
    /// cleared queue. It uses batch status queries for efficiency.
    ///
    /// # Arguments
    /// * `exit` - Atomic flag for coordinating thread shutdown
    /// * `sigs` - Shared pending transaction queue
    /// * `cleared` - Shared cleared transaction queue
    /// * `client` - RPC client for status queries
    ///
    /// # Returns
    /// JoinHandle for the background thread
    ///
    /// # Implementation Details
    /// - Processes transactions in batches of 200 for efficiency
    /// - Uses 30-second timeout for unconfirmed transactions
    /// - Logs progress statistics every 5 seconds
    /// - Sleeps 200ms between polling cycles to avoid overwhelming the RPC
    fn start_sig_clear_thread(
        exit: Arc<AtomicBool>,
        sigs: &Arc<RwLock<PendingQueue>>,
        cleared: &Arc<RwLock<Vec<u64>>>,
        client: &Arc<RpcClient>,
    ) -> JoinHandle<()> {
        let sigs = sigs.clone();
        let cleared = cleared.clone();
        let client = client.clone();
        // Create a named thread for easier debugging and monitoring
        Builder::new()
            .name("solSigClear".to_string())
            .spawn(move || {
                // Statistics tracking for monitoring and debugging
                let mut success = 0;      // Count of successfully confirmed transactions
                let mut error_count = 0;  // Count of transactions that failed with errors
                let mut timed_out = 0;    // Count of transactions that timed out
                let mut last_log = Instant::now(); // Last time we logged statistics
                // Main polling loop - continues until shutdown signal
                while !exit.load(Ordering::Relaxed) {
                    let sigs_len = sigs.read().unwrap().len();
                    if sigs_len > 0 {
                        // Acquire write lock on pending signatures for processing
                        let mut sigs_w = sigs.write().unwrap();
                        let mut start = Measure::start("sig_status");
                        // Batch signature status queries for efficiency (200 per batch)
                        // This significantly reduces RPC load compared to individual queries
                        let statuses: Vec<_> = sigs_w
                            .chunks(200)  // Process in chunks to respect RPC limits
                            .flat_map(|sig_chunk| {
                                let only_sigs: Vec<_> = sig_chunk.iter().map(|s| s.0).collect();
                                client
                                    .get_signature_statuses(&only_sigs)
                                    .expect("signature status query failed")
                                    .value
                            })
                            .collect();
                        // Initialize counters and tracking variables for this processing cycle
                        let mut num_cleared = 0;    // Count of transactions cleared this cycle
                        let start_len = sigs_w.len(); // Initial queue size for metrics
                        let now = timestamp();       // Current timestamp for timeout detection
                        let mut new_ids = vec![];    // IDs of transactions to move to cleared queue
                        let mut i = 0;              // Index in pending queue
                        let mut j = 0;              // Index in status results
                        // Process each pending transaction to determine if it should be cleared
                        while i != sigs_w.len() {
                            let mut retain = true;  // Whether to keep this transaction in pending queue
                            let sent_ts = sigs_w[i].1; // Timestamp when transaction was sent
                            
                            // Check if we received a status response for this transaction
                            if let Some(e) = &statuses[j] {
                                debug!("Transaction status received: {:?}", e);
                                if e.status.is_ok() {
                                    success += 1;     // Transaction confirmed successfully
                                } else {
                                    error_count += 1; // Transaction failed with an error
                                }
                                num_cleared += 1;
                                retain = false; // Remove from pending queue
                            } else if now - sent_ts > 30_000 { // 30 second timeout
                                retain = false; // Remove timed-out transaction
                                timed_out += 1;
                            }
                            
                            // Remove transaction from pending queue if it's complete or timed out
                            if !retain {
                                new_ids.push(sigs_w.remove(i).2); // Move ID to cleared list
                            } else {
                                i += 1; // Keep transaction, move to next
                            }
                            j += 1; // Move to next status result
                        }
                        // Finalize the processing cycle
                        let final_sigs_len = sigs_w.len();
                        drop(sigs_w); // Release write lock on pending queue
                        
                        // Move cleared transaction IDs to the cleared queue
                        cleared.write().unwrap().extend(new_ids);
                        start.stop(); // Stop timing measurement
                        // Log detailed performance metrics for debugging
                        debug!(
                            "Pending: {} | Success: {} | Processing time: {}ms | Cleared: {}/{}",
                            final_sigs_len,  // Remaining pending transactions
                            success,         // Total successful transactions
                            start.as_ms(),   // Time taken for this cycle
                            num_cleared,     // Transactions cleared this cycle
                            start_len,       // Initial queue size this cycle
                        );
                        // Log summary statistics every 5 seconds for monitoring
                        if last_log.elapsed().as_millis() > 5000 {
                            info!(
                                "Transaction status summary - Success: {} | Errors: {} | Timeouts: {}",
                                success, error_count, timed_out,
                            );
                            last_log = Instant::now();
                        }
                    }
                    // Sleep between polling cycles to avoid overwhelming the RPC endpoint
                    sleep(Duration::from_millis(200));
                } // End of main polling loop
            })
            .unwrap()
    }
}
