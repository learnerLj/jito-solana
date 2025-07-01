//! # Parallel Transaction Processing Module
//!
//! This module provides advanced parallel transaction processing capabilities with
//! automatic confirmation tracking, retry logic, and optimized batch handling.
//! It's designed for high-throughput scenarios where many transactions need to be
//! processed concurrently with reliable confirmation guarantees.
//!
//! ## Key Features
//!
//! - **Parallel Processing**: Concurrent transaction submission and confirmation
//! - **Automatic Retry Logic**: Smart retry mechanisms with exponential backoff
//! - **Blockhash Management**: Automatic blockhash refresh to prevent expiration
//! - **Progress Tracking**: Visual progress indicators for long-running operations
//! - **Error Handling**: Comprehensive error reporting per transaction
//! - **Fallback Strategies**: RPC fallback when TPU submission fails
//!
//! ## Architecture
//!
//! The module uses several concurrent tasks:
//! 1. **Blockhash Updater**: Continuously refreshes blockhashes to prevent expiration
//! 2. **Transaction Submitter**: Sends signed transactions to validators
//! 3. **Confirmation Tracker**: Monitors transaction status and handles confirmations
//! 4. **Retry Handler**: Manages retries for failed or expired transactions
//!
//! ## Performance Optimizations
//!
//! - Staggered transaction submission to avoid network congestion
//! - Batch status queries to reduce RPC load
//! - Efficient connection pooling with protocol selection (UDP/QUIC)
//! - Parallel confirmation tracking across multiple validator endpoints
//!
//! ## Usage Patterns
//!
//! This module is ideal for:
//! - Bulk transaction processing (airdrops, batch payments)
//! - High-frequency trading applications
//! - DeFi protocols requiring reliable transaction execution
//! - Any scenario requiring guaranteed transaction confirmation

use {
    crate::{
        nonblocking::{rpc_client::RpcClient, tpu_client::TpuClient},
        rpc_client::RpcClient as BlockingRpcClient,
    },
    bincode::serialize,
    dashmap::DashMap,
    futures_util::future::join_all,
    solana_hash::Hash,
    solana_message::Message,
    solana_quic_client::{QuicConfig, QuicConnectionManager, QuicPool},
    solana_rpc_client::spinner::{self, SendTransactionProgress},
    solana_rpc_client_api::{
        client_error::ErrorKind,
        config::RpcSendTransactionConfig,
        request::{RpcError, RpcResponseErrorData, MAX_GET_SIGNATURE_STATUSES_QUERY_ITEMS},
        response::RpcSimulateTransactionResult,
    },
    solana_signature::Signature,
    solana_signer::{signers::Signers, SignerError},
    solana_tpu_client::tpu_client::{Result, TpuSenderError},
    solana_transaction::Transaction,
    solana_transaction_error::TransactionError,
    std::{
        sync::{
            atomic::{AtomicU64, AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::{sync::RwLock, task::JoinHandle},
};

// Timing constants for optimal performance and reliability
const BLOCKHASH_REFRESH_RATE: Duration = Duration::from_secs(5);  // How often to refresh blockhashes
const SEND_INTERVAL: Duration = Duration::from_millis(10);        // Delay between individual tx sends
/// Timeout for TPU transaction fanout operations.
/// 
/// This "reasonable" constant represents the maximum time allowed for transaction
/// distribution across multiple TPU endpoints. Derived from empirical testing
/// in `solana_tpu_client::nonblocking::tpu_client::send_wire_transaction_futures`.
const SEND_TIMEOUT_INTERVAL: Duration = Duration::from_secs(5);

/// Type alias for QUIC-based TPU client used throughout this module.
/// QUIC provides better performance than UDP for high-volume transaction processing.
type QuicTpuClient = TpuClient<QuicPool, QuicConnectionManager, QuicConfig>;

/// Internal data structure for tracking individual transactions during processing.
///
/// This structure contains all necessary information to manage a transaction's
/// lifecycle from submission through confirmation or timeout.
#[derive(Clone, Debug)]
struct TransactionData {
    /// Block height after which this transaction becomes invalid
    last_valid_block_height: u64,
    /// Original message used to recreate the transaction if needed
    message: Message,
    /// Original index in the input batch for error reporting
    index: usize,
    /// Pre-serialized transaction bytes for efficient retransmission
    serialized_transaction: Vec<u8>,
}

/// Cached blockhash information for transaction signing.
///
/// This structure is updated regularly by the background blockhash refresh task
/// to ensure transactions always use valid, recent blockhashes.
#[derive(Clone, Debug, Copy)]
struct BlockHashData {
    /// Current blockhash for transaction signing
    pub blockhash: Hash,
    /// Block height after which this blockhash expires
    pub last_valid_block_height: u64,
}

/// Legacy configuration structure for backward compatibility.
///
/// This struct is deprecated in favor of `SendAndConfirmConfigV2` which provides
/// additional configuration options including RPC transaction submission settings.
#[deprecated(
    since = "2.2.0",
    note = "Use SendAndConfirmConfigV2 with send_and_confirm_transactions_in_parallel_v2"
)]
#[derive(Clone, Debug, Copy)]
pub struct SendAndConfirmConfig {
    /// Whether to display a progress spinner during processing
    pub with_spinner: bool,
    /// Optional limit on how many times to re-sign transactions with new blockhashes
    pub resign_txs_count: Option<usize>,
}

/// Enhanced configuration structure with comprehensive RPC settings.
///
/// This is the current configuration struct that should be used for new code.
/// It extends the legacy config with detailed RPC transaction submission options.
#[derive(Clone, Debug, Copy)]
pub struct SendAndConfirmConfigV2 {
    /// Whether to display a progress spinner during processing
    pub with_spinner: bool,
    /// Optional limit on transaction re-signing attempts with fresh blockhashes
    pub resign_txs_count: Option<usize>,
    /// Detailed configuration for RPC transaction submission (preflight, encoding, etc.)
    pub rpc_send_transaction_config: RpcSendTransactionConfig,
}

#[allow(deprecated)]
#[deprecated(
    since = "2.2.0",
    note = "Use send_and_confirm_transactions_in_parallel_v2"
)]
pub async fn send_and_confirm_transactions_in_parallel<T: Signers + ?Sized>(
    rpc_client: Arc<RpcClient>,
    tpu_client: Option<QuicTpuClient>,
    messages: &[Message],
    signers: &T,
    config: SendAndConfirmConfig,
) -> Result<Vec<Option<TransactionError>>> {
    let config_v2 = SendAndConfirmConfigV2 {
        with_spinner: config.with_spinner,
        resign_txs_count: config.resign_txs_count,
        rpc_send_transaction_config: RpcSendTransactionConfig {
            ..RpcSendTransactionConfig::default()
        },
    };
    send_and_confirm_transactions_in_parallel_v2(
        rpc_client, tpu_client, messages, signers, config_v2,
    )
    .await
}

#[allow(deprecated)]
#[deprecated(
    since = "2.2.0",
    note = "Use send_and_confirm_transactions_in_parallel_blocking_v2"
)]
pub fn send_and_confirm_transactions_in_parallel_blocking<T: Signers + ?Sized>(
    rpc_client: Arc<BlockingRpcClient>,
    tpu_client: Option<QuicTpuClient>,
    messages: &[Message],
    signers: &T,
    config: SendAndConfirmConfig,
) -> Result<Vec<Option<TransactionError>>> {
    let config_v2 = SendAndConfirmConfigV2 {
        with_spinner: config.with_spinner,
        resign_txs_count: config.resign_txs_count,
        rpc_send_transaction_config: RpcSendTransactionConfig {
            ..RpcSendTransactionConfig::default()
        },
    };
    send_and_confirm_transactions_in_parallel_blocking_v2(
        rpc_client, tpu_client, messages, signers, config_v2,
    )
}

/// Sends and confirms transactions concurrently in a sync context
pub fn send_and_confirm_transactions_in_parallel_blocking_v2<T: Signers + ?Sized>(
    rpc_client: Arc<BlockingRpcClient>,
    tpu_client: Option<QuicTpuClient>,
    messages: &[Message],
    signers: &T,
    config: SendAndConfirmConfigV2,
) -> Result<Vec<Option<TransactionError>>> {
    let fut = send_and_confirm_transactions_in_parallel_v2(
        rpc_client.get_inner_client().clone(),
        tpu_client,
        messages,
        signers,
        config,
    );
    tokio::task::block_in_place(|| rpc_client.runtime().block_on(fut))
}

fn create_blockhash_data_updating_task(
    rpc_client: Arc<RpcClient>,
    blockhash_data_rw: Arc<RwLock<BlockHashData>>,
    current_block_height: Arc<AtomicU64>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Ok((blockhash, last_valid_block_height)) = rpc_client
                .get_latest_blockhash_with_commitment(rpc_client.commitment())
                .await
            {
                *blockhash_data_rw.write().await = BlockHashData {
                    blockhash,
                    last_valid_block_height,
                };
            }

            if let Ok(block_height) = rpc_client.get_block_height().await {
                current_block_height.store(block_height, Ordering::Relaxed);
            }
            tokio::time::sleep(BLOCKHASH_REFRESH_RATE).await;
        }
    })
}

fn create_transaction_confirmation_task(
    rpc_client: Arc<RpcClient>,
    current_block_height: Arc<AtomicU64>,
    unconfirmed_transaction_map: Arc<DashMap<Signature, TransactionData>>,
    errors_map: Arc<DashMap<usize, TransactionError>>,
    num_confirmed_transactions: Arc<AtomicUsize>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // check transactions that are not expired or have just expired between two checks
        let mut last_block_height = current_block_height.load(Ordering::Relaxed);

        loop {
            if !unconfirmed_transaction_map.is_empty() {
                let current_block_height = current_block_height.load(Ordering::Relaxed);
                let transactions_to_verify: Vec<Signature> = unconfirmed_transaction_map
                    .iter()
                    .filter(|x| {
                        let is_not_expired = current_block_height <= x.last_valid_block_height;
                        // transaction expired between last and current check
                        let is_recently_expired = last_block_height <= x.last_valid_block_height
                            && current_block_height > x.last_valid_block_height;
                        is_not_expired || is_recently_expired
                    })
                    .map(|x| *x.key())
                    .collect();
                for signatures in
                    transactions_to_verify.chunks(MAX_GET_SIGNATURE_STATUSES_QUERY_ITEMS)
                {
                    if let Ok(result) = rpc_client.get_signature_statuses(signatures).await {
                        let statuses = result.value;
                        for (signature, status) in signatures.iter().zip(statuses.into_iter()) {
                            if let Some((status, data)) = status
                                .filter(|status| {
                                    status.satisfies_commitment(rpc_client.commitment())
                                })
                                .and_then(|status| {
                                    unconfirmed_transaction_map
                                        .remove(signature)
                                        .map(|(_, data)| (status, data))
                                })
                            {
                                num_confirmed_transactions.fetch_add(1, Ordering::Relaxed);
                                match status.err {
                                    Some(TransactionError::AlreadyProcessed) | None => {}
                                    Some(error) => {
                                        errors_map.insert(data.index, error);
                                    }
                                }
                            };
                        }
                    }
                }

                last_block_height = current_block_height;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}

/// Shared context for coordinating parallel transaction processing.
///
/// This structure contains all shared state needed to coordinate between
/// the various async tasks handling transaction submission, confirmation,
/// and status tracking.
#[derive(Clone, Debug)]
struct SendingContext {
    /// Map of pending transactions awaiting confirmation, keyed by signature
    unconfirmed_transaction_map: Arc<DashMap<Signature, TransactionData>>,
    /// Map of transaction errors, keyed by original batch index
    error_map: Arc<DashMap<usize, TransactionError>>,
    /// Current blockhash data for transaction signing
    blockhash_data_rw: Arc<RwLock<BlockHashData>>,
    /// Running count of confirmed transactions for progress tracking
    num_confirmed_transactions: Arc<AtomicUsize>,
    /// Total number of transactions being processed
    total_transactions: usize,
    /// Current blockchain height for timeout detection
    current_block_height: Arc<AtomicU64>,
}
fn progress_from_context_and_block_height(
    context: &SendingContext,
    last_valid_block_height: u64,
) -> SendTransactionProgress {
    SendTransactionProgress {
        confirmed_transactions: context
            .num_confirmed_transactions
            .load(std::sync::atomic::Ordering::Relaxed),
        total_transactions: context.total_transactions,
        block_height: context
            .current_block_height
            .load(std::sync::atomic::Ordering::Relaxed),
        last_valid_block_height,
    }
}

async fn send_transaction_with_rpc_fallback(
    rpc_client: &RpcClient,
    tpu_client: &Option<QuicTpuClient>,
    transaction: Transaction,
    serialized_transaction: Vec<u8>,
    context: &SendingContext,
    index: usize,
    rpc_send_transaction_config: RpcSendTransactionConfig,
) -> Result<()> {
    let send_over_rpc = if let Some(tpu_client) = tpu_client {
        !tokio::time::timeout(
            SEND_TIMEOUT_INTERVAL,
            tpu_client.send_wire_transaction(serialized_transaction.clone()),
        )
        .await
        .unwrap_or(false)
    } else {
        true
    };
    if send_over_rpc {
        if let Err(e) = rpc_client
            .send_transaction_with_config(
                &transaction,
                RpcSendTransactionConfig {
                    preflight_commitment: Some(rpc_client.commitment().commitment),
                    ..rpc_send_transaction_config
                },
            )
            .await
        {
            match e.kind() {
                ErrorKind::Io(_) | ErrorKind::Reqwest(_) => {
                    // fall through on io error, we will retry the transaction
                }
                ErrorKind::TransactionError(TransactionError::BlockhashNotFound)
                | ErrorKind::RpcError(RpcError::RpcResponseError {
                    data:
                        RpcResponseErrorData::SendTransactionPreflightFailure(
                            RpcSimulateTransactionResult {
                                err: Some(TransactionError::BlockhashNotFound),
                                ..
                            },
                        ),
                    ..
                }) => {
                    // fall through so that we will resend with another blockhash
                }
                ErrorKind::TransactionError(transaction_error)
                | ErrorKind::RpcError(RpcError::RpcResponseError {
                    data:
                        RpcResponseErrorData::SendTransactionPreflightFailure(
                            RpcSimulateTransactionResult {
                                err: Some(transaction_error),
                                ..
                            },
                        ),
                    ..
                }) => {
                    // if we get other than blockhash not found error the transaction is invalid
                    context.error_map.insert(index, transaction_error.clone());
                }
                _ => {
                    return Err(TpuSenderError::from(e));
                }
            }
        }
    }
    Ok(())
}

async fn sign_all_messages_and_send<T: Signers + ?Sized>(
    progress_bar: &Option<indicatif::ProgressBar>,
    rpc_client: &RpcClient,
    tpu_client: &Option<QuicTpuClient>,
    messages_with_index: Vec<(usize, Message)>,
    signers: &T,
    context: &SendingContext,
    rpc_send_transaction_config: RpcSendTransactionConfig,
) -> Result<()> {
    let current_transaction_count = messages_with_index.len();
    let mut futures = vec![];
    // send all the transaction messages
    for (counter, (index, message)) in messages_with_index.iter().enumerate() {
        let mut transaction = Transaction::new_unsigned(message.clone());
        futures.push(async move {
            tokio::time::sleep(SEND_INTERVAL.saturating_mul(counter as u32)).await;
            let blockhashdata = *context.blockhash_data_rw.read().await;

            // we have already checked if all transactions are signable.
            transaction
                .try_sign(signers, blockhashdata.blockhash)
                .expect("Transaction should be signable");
            let serialized_transaction =
                serialize(&transaction).expect("Transaction should serialize");
            let signature = transaction.signatures[0];

            // send to confirm the transaction
            context.unconfirmed_transaction_map.insert(
                signature,
                TransactionData {
                    index: *index,
                    serialized_transaction: serialized_transaction.clone(),
                    last_valid_block_height: blockhashdata.last_valid_block_height,
                    message: message.clone(),
                },
            );
            if let Some(progress_bar) = progress_bar {
                let progress = progress_from_context_and_block_height(
                    context,
                    blockhashdata.last_valid_block_height,
                );
                progress.set_message_for_confirmed_transactions(
                    progress_bar,
                    &format!(
                        "Sending {}/{} transactions",
                        counter + 1,
                        current_transaction_count,
                    ),
                );
            }
            send_transaction_with_rpc_fallback(
                rpc_client,
                tpu_client,
                transaction,
                serialized_transaction,
                context,
                *index,
                rpc_send_transaction_config,
            )
            .await
        });
    }
    // collect to convert Vec<Result<_>> to Result<Vec<_>>
    join_all(futures)
        .await
        .into_iter()
        .collect::<Result<Vec<()>>>()?;
    Ok(())
}

async fn confirm_transactions_till_block_height_and_resend_unexpired_transaction_over_tpu(
    progress_bar: &Option<indicatif::ProgressBar>,
    tpu_client: &Option<QuicTpuClient>,
    context: &SendingContext,
) {
    let unconfirmed_transaction_map = context.unconfirmed_transaction_map.clone();
    let current_block_height = context.current_block_height.clone();

    let transactions_to_confirm = unconfirmed_transaction_map.len();
    let max_valid_block_height = unconfirmed_transaction_map
        .iter()
        .map(|x| x.last_valid_block_height)
        .max();

    if let Some(mut max_valid_block_height) = max_valid_block_height {
        if let Some(progress_bar) = progress_bar {
            let progress = progress_from_context_and_block_height(context, max_valid_block_height);
            progress.set_message_for_confirmed_transactions(
                progress_bar,
                &format!(
                    "Waiting for next block, {transactions_to_confirm} transactions pending..."
                ),
            );
        }

        // wait till all transactions are confirmed or we have surpassed max processing age for the last sent transaction
        while !unconfirmed_transaction_map.is_empty()
            && current_block_height.load(Ordering::Relaxed) <= max_valid_block_height
        {
            let block_height = current_block_height.load(Ordering::Relaxed);

            if let Some(tpu_client) = tpu_client {
                // retry sending transaction only over TPU port
                // any transactions sent over RPC will be automatically rebroadcast by the RPC server
                let txs_to_resend_over_tpu = unconfirmed_transaction_map
                    .iter()
                    .filter(|x| block_height < x.last_valid_block_height)
                    .map(|x| x.serialized_transaction.clone())
                    .collect::<Vec<_>>();
                send_staggered_transactions(
                    progress_bar,
                    tpu_client,
                    txs_to_resend_over_tpu,
                    max_valid_block_height,
                    context,
                )
                .await;
            } else {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if let Some(max_valid_block_height_in_remaining_transaction) =
                unconfirmed_transaction_map
                    .iter()
                    .map(|x| x.last_valid_block_height)
                    .max()
            {
                max_valid_block_height = max_valid_block_height_in_remaining_transaction;
            }
        }
    }
}

async fn send_staggered_transactions(
    progress_bar: &Option<indicatif::ProgressBar>,
    tpu_client: &QuicTpuClient,
    wire_transactions: Vec<Vec<u8>>,
    last_valid_block_height: u64,
    context: &SendingContext,
) {
    let current_transaction_count = wire_transactions.len();
    let futures = wire_transactions
        .into_iter()
        .enumerate()
        .map(|(counter, transaction)| async move {
            tokio::time::sleep(SEND_INTERVAL.saturating_mul(counter as u32)).await;
            if let Some(progress_bar) = progress_bar {
                let progress =
                    progress_from_context_and_block_height(context, last_valid_block_height);
                progress.set_message_for_confirmed_transactions(
                    progress_bar,
                    &format!(
                        "Resending {}/{} transactions",
                        counter + 1,
                        current_transaction_count,
                    ),
                );
            }
            tokio::time::timeout(
                SEND_TIMEOUT_INTERVAL,
                tpu_client.send_wire_transaction(transaction),
            )
            .await
        })
        .collect::<Vec<_>>();
    join_all(futures).await;
}

/// Processes transactions in parallel with comprehensive confirmation tracking.
///
/// This is the main entry point for high-performance transaction processing.
/// It coordinates multiple async tasks to handle transaction submission,
/// confirmation tracking, and retry logic with optimal efficiency.
///
/// ## Process Flow
///
/// 1. **Pre-flight validation**: Verify all messages can be signed
/// 2. **Background task setup**: Start blockhash refresh and confirmation tracking
/// 3. **Batch processing**: Sign and submit transactions in parallel
/// 4. **Confirmation monitoring**: Track transaction status until confirmed or expired
/// 5. **Retry logic**: Re-sign and resubmit expired transactions with fresh blockhashes
///
/// ## Performance Features
///
/// - **Staggered submission**: Prevents network congestion
/// - **Batch confirmation**: Efficient status queries for multiple transactions
/// - **TPU + RPC hybrid**: Uses TPU for speed, RPC for reliability
/// - **Smart retries**: Only retries transactions that may still succeed
///
/// # Arguments
/// * `rpc_client` - RPC client for blockchain queries and fallback submission
/// * `tpu_client` - Optional TPU client for high-performance transaction submission
/// * `messages` - Array of messages to process as transactions
/// * `signers` - Collection of signers for transaction authorization
/// * `config` - Configuration parameters for processing behavior
///
/// # Returns
/// Vector of optional transaction errors (None = success, Some(err) = failure)
///
/// # Errors
/// Returns error if:
/// - Initial blockhash fetch fails
/// - Message signing validation fails
/// - Maximum retry attempts exceeded
pub async fn send_and_confirm_transactions_in_parallel_v2<T: Signers + ?Sized>(
    rpc_client: Arc<RpcClient>,
    tpu_client: Option<QuicTpuClient>,
    messages: &[Message],
    signers: &T,
    config: SendAndConfirmConfigV2,
) -> Result<Vec<Option<TransactionError>>> {
    // get current blockhash and corresponding last valid block height
    let (blockhash, last_valid_block_height) = rpc_client
        .get_latest_blockhash_with_commitment(rpc_client.commitment())
        .await?;
    let blockhash_data_rw = Arc::new(RwLock::new(BlockHashData {
        blockhash,
        last_valid_block_height,
    }));

    // check if all the messages are signable by the signers
    messages
        .iter()
        .map(|x| {
            let mut transaction = Transaction::new_unsigned(x.clone());
            transaction.try_sign(signers, blockhash)
        })
        .collect::<std::result::Result<Vec<()>, SignerError>>()?;

    // get current block height
    let block_height = rpc_client.get_block_height().await?;
    let current_block_height = Arc::new(AtomicU64::new(block_height));

    let progress_bar = config.with_spinner.then(|| {
        let progress_bar = spinner::new_progress_bar();
        progress_bar.set_message("Setting up...");
        progress_bar
    });

    // blockhash and block height update task
    let block_data_task = create_blockhash_data_updating_task(
        rpc_client.clone(),
        blockhash_data_rw.clone(),
        current_block_height.clone(),
    );

    let unconfirmed_transasction_map = Arc::new(DashMap::<Signature, TransactionData>::new());
    let error_map = Arc::new(DashMap::new());
    let num_confirmed_transactions = Arc::new(AtomicUsize::new(0));
    // tasks which confirms the transactions that were sent
    let transaction_confirming_task = create_transaction_confirmation_task(
        rpc_client.clone(),
        current_block_height.clone(),
        unconfirmed_transasction_map.clone(),
        error_map.clone(),
        num_confirmed_transactions.clone(),
    );

    // transaction sender task
    let total_transactions = messages.len();
    let mut initial = true;
    let signing_count = config.resign_txs_count.unwrap_or(1);
    let context = SendingContext {
        unconfirmed_transaction_map: unconfirmed_transasction_map.clone(),
        blockhash_data_rw: blockhash_data_rw.clone(),
        num_confirmed_transactions: num_confirmed_transactions.clone(),
        current_block_height: current_block_height.clone(),
        error_map: error_map.clone(),
        total_transactions,
    };

    for expired_blockhash_retries in (0..signing_count).rev() {
        // only send messages which have not been confirmed
        let messages_with_index: Vec<(usize, Message)> = if initial {
            initial = false;
            messages.iter().cloned().enumerate().collect()
        } else {
            // remove all the confirmed transactions
            unconfirmed_transasction_map
                .iter()
                .map(|x| (x.index, x.message.clone()))
                .collect()
        };

        if messages_with_index.is_empty() {
            break;
        }

        // clear the map so that we can start resending
        unconfirmed_transasction_map.clear();

        sign_all_messages_and_send(
            &progress_bar,
            &rpc_client,
            &tpu_client,
            messages_with_index,
            signers,
            &context,
            config.rpc_send_transaction_config,
        )
        .await?;
        confirm_transactions_till_block_height_and_resend_unexpired_transaction_over_tpu(
            &progress_bar,
            &tpu_client,
            &context,
        )
        .await;

        if unconfirmed_transasction_map.is_empty() {
            break;
        }

        if let Some(progress_bar) = &progress_bar {
            progress_bar.println(format!(
                "Blockhash expired. {expired_blockhash_retries} retries remaining"
            ));
        }
    }

    block_data_task.abort();
    transaction_confirming_task.abort();
    if unconfirmed_transasction_map.is_empty() {
        let mut transaction_errors = vec![None; messages.len()];
        for iterator in error_map.iter() {
            transaction_errors[*iterator.key()] = Some(iterator.value().clone());
        }
        Ok(transaction_errors)
    } else {
        Err(TpuSenderError::Custom("Max retries exceeded".into()))
    }
}
