//! # Thin Client Module
//!
//! The `thin_client` module provides a lightweight client interface that combines
//! RPC queries with direct TPU (Transaction Processing Unit) communication.
//! This hybrid approach allows applications to query blockchain state via RPC
//! while submitting transactions directly to the TPU for better performance.
//!
//! ## Architecture
//!
//! The thin client operates on two communication channels:
//! - **RPC Channel**: Used for querying blockchain state, account data, and transaction status
//! - **TPU Channel**: Used for direct transaction submission to bypass RPC bottlenecks
//!
//! ## Key Features
//!
//! - **Protocol Abstraction**: Supports both UDP and QUIC for TPU communication
//! - **Automatic Retry Logic**: Built-in retry mechanisms for failed transactions
//! - **Confirmation Tracking**: Monitors transaction confirmation status
//! - **Balance Polling**: Efficient account balance monitoring
//!
//! ## Usage Patterns
//!
//! The thin client is ideal for applications that need:
//! - High-frequency transaction submission
//! - Real-time account monitoring
//! - Direct validator communication
//! - Minimal client-side overhead
//!
//! ## Warning
//!
//! The binary encoding of TPU messages is unstable and may change in future releases.
//! Client code should use this interface rather than communicating with the TPU directly.
#[allow(deprecated)]
use {
    crate::connection_cache::{dispatch, ConnectionCache},
    solana_account::Account,
    solana_client_traits::{AsyncClient, Client, SyncClient},
    solana_commitment_config::CommitmentConfig,
    solana_epoch_info::EpochInfo,
    solana_hash::Hash,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_message::Message,
    solana_pubkey::Pubkey,
    solana_quic_client::{QuicConfig, QuicConnectionManager, QuicPool},
    solana_rpc_client::rpc_client::RpcClient,
    solana_rpc_client_api::config::RpcProgramAccountsConfig,
    solana_signature::Signature,
    solana_signer::signers::Signers,
    solana_thin_client::thin_client::ThinClient as BackendThinClient,
    solana_transaction::{versioned::VersionedTransaction, Transaction},
    solana_transaction_error::{TransactionResult, TransportResult},
    solana_udp_client::{UdpConfig, UdpConnectionManager, UdpPool},
    std::{net::SocketAddr, sync::Arc, time::Duration},
};

/// A protocol-agnostic thin client that abstracts UDP and QUIC communication.
///
/// This enum provides a unified interface for thin client operations regardless of
/// the underlying transport protocol. It automatically handles protocol-specific
/// optimizations and failover scenarios.
///
/// # Protocol Selection
///
/// - **QUIC**: Preferred for better performance, multiplexing, and built-in encryption
/// - **UDP**: Used for backward compatibility or when QUIC is unavailable
///
/// # Usage
///
/// ```rust
/// // Create a thin client with automatic protocol selection
/// let client = ThinClient::new(rpc_addr, tpu_addr, connection_cache);
///
/// // Send and confirm a transaction
/// let signature = client.send_and_confirm_transaction(&signers, &mut transaction, 5, 0)?;
/// ```
///
/// For scenarios using only UDP or QUIC, consider using the backend ThinClient directly
/// for reduced abstraction overhead.
#[allow(deprecated)]
pub enum ThinClient {
    /// QUIC-based thin client with advanced features like multiplexing and encryption
    Quic(BackendThinClient<QuicPool, QuicConnectionManager, QuicConfig>),
    /// UDP-based thin client for simple, stateless communication
    Udp(BackendThinClient<UdpPool, UdpConnectionManager, UdpConfig>),
}

#[allow(deprecated)]
impl ThinClient {
    /// Creates a new ThinClient with RPC and TPU endpoints.
    ///
    /// The client will use TCP for RPC communication and either QUIC or UDP for TPU
    /// communication based on the connection cache configuration.
    ///
    /// # Arguments
    /// * `rpc_addr` - Socket address for RPC server (blockchain queries)
    /// * `tpu_addr` - Socket address for TPU server (transaction submission)
    /// * `connection_cache` - Shared connection cache determining protocol (UDP/QUIC)
    ///
    /// # Returns
    /// A configured `ThinClient` ready for blockchain operations
    pub fn new(
        rpc_addr: SocketAddr,
        tpu_addr: SocketAddr,
        connection_cache: Arc<ConnectionCache>,
    ) -> Self {
        match &*connection_cache {
            ConnectionCache::Quic(connection_cache) => {
                let thin_client =
                    BackendThinClient::new(rpc_addr, tpu_addr, connection_cache.clone());
                ThinClient::Quic(thin_client)
            }
            ConnectionCache::Udp(connection_cache) => {
                let thin_client =
                    BackendThinClient::new(rpc_addr, tpu_addr, connection_cache.clone());
                ThinClient::Udp(thin_client)
            }
        }
    }

    /// Creates a new ThinClient with custom timeout settings.
    ///
    /// This method allows fine-tuning of network timeouts for both RPC and TPU
    /// operations, which is useful in high-latency or congested network environments.
    ///
    /// # Arguments
    /// * `rpc_addr` - Socket address for RPC server
    /// * `tpu_addr` - Socket address for TPU server  
    /// * `timeout` - Custom timeout duration for network operations
    /// * `connection_cache` - Shared connection cache for protocol selection
    ///
    /// # Returns
    /// A configured `ThinClient` with custom timeout settings
    pub fn new_socket_with_timeout(
        rpc_addr: SocketAddr,
        tpu_addr: SocketAddr,
        timeout: Duration,
        connection_cache: Arc<ConnectionCache>,
    ) -> Self {
        match &*connection_cache {
            ConnectionCache::Quic(connection_cache) => {
                let thin_client = BackendThinClient::new_socket_with_timeout(
                    rpc_addr,
                    tpu_addr,
                    timeout,
                    connection_cache.clone(),
                );
                ThinClient::Quic(thin_client)
            }
            ConnectionCache::Udp(connection_cache) => {
                let thin_client = BackendThinClient::new_socket_with_timeout(
                    rpc_addr,
                    tpu_addr,
                    timeout,
                    connection_cache.clone(),
                );
                ThinClient::Udp(thin_client)
            }
        }
    }

    /// Creates a new ThinClient with multiple RPC and TPU endpoints.
    ///
    /// This method enables load balancing and failover across multiple validator
    /// endpoints. The client will automatically distribute requests and handle
    /// endpoint failures gracefully.
    ///
    /// # Arguments
    /// * `rpc_addrs` - List of RPC server addresses for load balancing
    /// * `tpu_addrs` - List of TPU server addresses for transaction distribution
    /// * `connection_cache` - Shared connection cache for protocol management
    ///
    /// # Returns
    /// A multi-endpoint `ThinClient` with built-in failover capabilities
    pub fn new_from_addrs(
        rpc_addrs: Vec<SocketAddr>,
        tpu_addrs: Vec<SocketAddr>,
        connection_cache: Arc<ConnectionCache>,
    ) -> Self {
        match &*connection_cache {
            ConnectionCache::Quic(connection_cache) => {
                let thin_client = BackendThinClient::new_from_addrs(
                    rpc_addrs,
                    tpu_addrs,
                    connection_cache.clone(),
                );
                ThinClient::Quic(thin_client)
            }
            ConnectionCache::Udp(connection_cache) => {
                let thin_client = BackendThinClient::new_from_addrs(
                    rpc_addrs,
                    tpu_addrs,
                    connection_cache.clone(),
                );
                ThinClient::Udp(thin_client)
            }
        }
    }

    /// Returns a reference to the underlying RPC client.
    ///
    /// This provides access to the RPC client for direct blockchain queries
    /// when thin client methods don't cover specific use cases.
    dispatch!(pub fn rpc_client(&self) -> &RpcClient);

    /// Retries a transfer transaction until it's confirmed on the blockchain.
    ///
    /// This method continuously retries a transfer transaction until it receives
    /// the required number of confirmations or exhausts the retry limit.
    ///
    /// # Arguments
    /// * `keypair` - The keypair for signing the transaction
    /// * `transaction` - The transfer transaction to retry
    /// * `tries` - Maximum number of retry attempts
    /// * `min_confirmed_blocks` - Minimum confirmations required
    ///
    /// # Returns
    /// The transaction signature upon successful confirmation
    dispatch!(pub fn retry_transfer_until_confirmed(&self, keypair: &Keypair, transaction: &mut Transaction, tries: usize, min_confirmed_blocks: usize) -> TransportResult<Signature>);

    /// Retries a transfer transaction with specified attempts.
    ///
    /// Similar to retry_transfer_until_confirmed but without waiting for
    /// specific confirmation levels. Useful for fire-and-forget scenarios.
    ///
    /// # Arguments
    /// * `keypair` - The keypair for signing the transaction
    /// * `transaction` - The transfer transaction to retry
    /// * `tries` - Maximum number of retry attempts
    ///
    /// # Returns
    /// The transaction signature upon successful submission
    dispatch!(pub fn retry_transfer(
        &self,
        keypair: &Keypair,
        transaction: &mut Transaction,
        tries: usize
    ) -> TransportResult<Signature>);

    /// Sends a transaction and waits for confirmation.
    ///
    /// This is the primary method for transaction submission with confirmation.
    /// It handles signing, submission, and confirmation tracking automatically.
    ///
    /// # Arguments
    /// * `keypairs` - Collection of keypairs for multi-sig transactions
    /// * `transaction` - The transaction to send and confirm
    /// * `tries` - Maximum retry attempts for submission
    /// * `pending_confirmations` - Number of confirmations to wait for
    ///
    /// # Returns
    /// The transaction signature upon successful confirmation
    dispatch!(pub fn send_and_confirm_transaction<T: Signers + ?Sized>(
        &self,
        keypairs: &T,
        transaction: &mut Transaction,
        tries: usize,
        pending_confirmations: usize
    ) -> TransportResult<Signature>);

    /// Polls for the current balance of an account.
    ///
    /// This method actively polls the validator until it receives a response,
    /// making it suitable for real-time balance monitoring.
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the account to query
    ///
    /// # Returns
    /// The account balance in lamports
    dispatch!(pub fn poll_get_balance(&self, pubkey: &Pubkey) -> TransportResult<u64>);

    /// Polls for account balance with specified commitment level.
    ///
    /// Allows specifying the commitment level for balance queries, providing
    /// control over the trade-off between speed and finality.
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the account to query
    /// * `commitment_config` - The commitment level (confirmed, finalized, etc.)
    ///
    /// # Returns
    /// The account balance in lamports at the specified commitment level
    dispatch!(pub fn poll_get_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig
    ) -> TransportResult<u64>);

    /// Waits for an account to reach an expected balance.
    ///
    /// This method blocks until the account balance matches the expected value
    /// or a timeout occurs. Useful for synchronizing on balance changes.
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the account to monitor
    /// * `expected_balance` - The balance to wait for (None = any balance change)
    ///
    /// # Returns
    /// The actual balance when condition is met, or None on timeout
    dispatch!(pub fn wait_for_balance(&self, pubkey: &Pubkey, expected_balance: Option<u64>) -> Option<u64>);

    dispatch!(pub fn get_program_accounts_with_config(
        &self,
        pubkey: &Pubkey,
        config: RpcProgramAccountsConfig
    ) -> TransportResult<Vec<(Pubkey, Account)>>);

    dispatch!(pub fn wait_for_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        expected_balance: Option<u64>,
        commitment_config: CommitmentConfig
    ) -> Option<u64>);

    dispatch!(pub fn poll_for_signature_with_commitment(
        &self,
        signature: &Signature,
        commitment_config: CommitmentConfig
    ) -> TransportResult<()>);

    dispatch!(pub fn get_num_blocks_since_signature_confirmation(
        &mut self,
        sig: &Signature
    ) -> TransportResult<usize>);
}

/// Implementation of the basic Client trait.
///
/// Provides fundamental client capabilities that are common across
/// different client implementations in the Solana ecosystem.
impl Client for ThinClient {
    /// Returns the TPU address as a string.
    ///
    /// This method exposes the TPU endpoint address for diagnostic
    /// or routing purposes.
    dispatch!(fn tpu_addr(&self) -> String);
}

/// Implementation of synchronous client operations.
///
/// This trait provides blocking versions of common blockchain operations
/// like transaction submission, account queries, and balance checks.
/// All methods in this implementation block until completion.
impl SyncClient for ThinClient {
    /// Sends and confirms a message-based transaction.
    ///
    /// This method takes a pre-built message, signs it with the provided keypairs,
    /// submits it to the network, and waits for confirmation.
    ///
    /// # Arguments
    /// * `keypairs` - Collection of keypairs for signing
    /// * `message` - The message to send as a transaction
    ///
    /// # Returns
    /// The transaction signature upon confirmation
    dispatch!(fn send_and_confirm_message<T: Signers + ?Sized>(
        &self,
        keypairs: &T,
        message: Message
    ) -> TransportResult<Signature>);

    /// Sends and confirms a single instruction as a transaction.
    ///
    /// This convenience method wraps a single instruction in a transaction,
    /// handles signing and submission, and waits for confirmation.
    ///
    /// # Arguments
    /// * `keypair` - The keypair for signing the transaction
    /// * `instruction` - The instruction to execute
    ///
    /// # Returns
    /// The transaction signature upon confirmation
    dispatch!(fn send_and_confirm_instruction(
        &self,
        keypair: &Keypair,
        instruction: Instruction
    ) -> TransportResult<Signature>);

    /// Transfers lamports from one account to another with confirmation.
    ///
    /// This is a high-level convenience method for SOL transfers that handles
    /// transaction creation, signing, submission, and confirmation.
    ///
    /// # Arguments
    /// * `lamports` - Amount to transfer in lamports (1 SOL = 1e9 lamports)
    /// * `keypair` - Source account keypair (sender)
    /// * `pubkey` - Destination account public key (recipient)
    ///
    /// # Returns
    /// The transaction signature upon successful transfer confirmation
    dispatch!(fn transfer_and_confirm(
        &self,
        lamports: u64,
        keypair: &Keypair,
        pubkey: &Pubkey
    ) -> TransportResult<Signature>);

    /// Retrieves the raw data stored in an account.
    ///
    /// Returns the account's data field, which contains program-specific
    /// information. Returns None if the account doesn't exist.
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the account to query
    ///
    /// # Returns
    /// Optional account data as bytes, or None if account doesn't exist
    dispatch!(fn get_account_data(&self, pubkey: &Pubkey) -> TransportResult<Option<Vec<u8>>>);

    /// Retrieves complete account information.
    ///
    /// Returns the full account structure including balance, owner, data,
    /// and metadata. This is more comprehensive than get_account_data.
    ///
    /// # Arguments
    /// * `pubkey` - The public key of the account to query
    ///
    /// # Returns
    /// Optional complete account information, or None if account doesn't exist
    dispatch!(fn get_account(&self, pubkey: &Pubkey) -> TransportResult<Option<Account>>);

    dispatch!(fn get_account_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig
    ) -> TransportResult<Option<Account>>);

    dispatch!(fn get_balance(&self, pubkey: &Pubkey) -> TransportResult<u64>);

    dispatch!(fn get_balance_with_commitment(
        &self,
        pubkey: &Pubkey,
        commitment_config: CommitmentConfig
    ) -> TransportResult<u64>);

    dispatch!(fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> TransportResult<u64>);

    dispatch!(fn get_signature_status(
        &self,
        signature: &Signature
    ) -> TransportResult<Option<TransactionResult<()>>>);

    dispatch!(fn get_signature_status_with_commitment(
        &self,
        signature: &Signature,
        commitment_config: CommitmentConfig
    ) -> TransportResult<Option<TransactionResult<()>>>);

    dispatch!(fn get_slot(&self) -> TransportResult<u64>);

    dispatch!(fn get_slot_with_commitment(
        &self,
        commitment_config: CommitmentConfig
    ) -> TransportResult<u64>);

    dispatch!(fn get_epoch_info(&self) -> TransportResult<EpochInfo>);

    dispatch!(fn get_transaction_count(&self) -> TransportResult<u64>);

    dispatch!(fn get_transaction_count_with_commitment(
        &self,
        commitment_config: CommitmentConfig
    ) -> TransportResult<u64>);

    dispatch!(fn poll_for_signature_confirmation(
        &self,
        signature: &Signature,
        min_confirmed_blocks: usize
    ) -> TransportResult<usize>);

    dispatch!(fn poll_for_signature(&self, signature: &Signature) -> TransportResult<()>);

    dispatch!(fn get_latest_blockhash(&self) -> TransportResult<Hash>);

    dispatch!(fn get_latest_blockhash_with_commitment(
        &self,
        commitment_config: CommitmentConfig
    ) -> TransportResult<(Hash, u64)>);

    dispatch!(fn is_blockhash_valid(
        &self,
        blockhash: &Hash,
        commitment_config: CommitmentConfig
    ) -> TransportResult<bool>);

    dispatch!(fn get_fee_for_message(&self, message: &Message) -> TransportResult<u64>);
}

/// Implementation of asynchronous client operations.
///
/// This trait provides async versions of transaction operations that don't
/// wait for confirmation. These methods are optimized for high-throughput
/// scenarios where confirmation tracking is handled separately.
impl AsyncClient for ThinClient {
    /// Asynchronously sends a versioned transaction without waiting for confirmation.
    ///
    /// This method submits a versioned transaction to the network and immediately
    /// returns the signature. It doesn't wait for confirmation, making it suitable
    /// for high-throughput applications.
    ///
    /// # Arguments
    /// * `transaction` - The versioned transaction to send
    ///
    /// # Returns
    /// The transaction signature immediately upon submission
    dispatch!(fn async_send_versioned_transaction(
        &self,
        transaction: VersionedTransaction
    ) -> TransportResult<Signature>);

    /// Asynchronously sends a batch of versioned transactions.
    ///
    /// This method submits multiple transactions in a single batch operation,
    /// optimizing network utilization and reducing latency for bulk operations.
    ///
    /// # Arguments
    /// * `batch` - Vector of versioned transactions to send
    ///
    /// # Returns
    /// Success indicator for the batch submission
    dispatch!(fn async_send_versioned_transaction_batch(
        &self,
        batch: Vec<VersionedTransaction>
    ) -> TransportResult<()>);
}
