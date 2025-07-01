//! BigTable Upload - Cloud Archival Integration for Ledger Data
//! 
//! This module implements high-performance upload functionality for archiving Solana
//! blockchain data to Google Cloud BigTable, providing long-term storage, backup,
//! and analytics capabilities. The upload system is designed for efficient batch
//! processing while maintaining data integrity and minimizing impact on validator performance.
//! 
//! ## Core Functionality
//! 
//! - **Block Upload**: Efficient transfer of confirmed blocks to cloud storage
//! - **Parallel Processing**: Concurrent uploads for maximum throughput
//! - **Read-Ahead Optimization**: Intelligent prefetching to minimize I/O latency
//! - **Deduplication**: Avoids redundant uploads of already-archived data
//! - **Progress Tracking**: Comprehensive monitoring of upload operations
//! 
//! ## Performance Optimizations
//! 
//! - **Batch Processing**: Groups multiple blocks for efficient network utilization
//! - **Parallel Uploads**: Configurable concurrency based on system resources
//! - **Read-Ahead Pipeline**: Overlaps block reading with upload operations
//! - **Connection Pooling**: Reuses network connections for reduced overhead
//! - **Compression**: Optimizes data transfer size and storage costs
//! 
//! ## Configuration Options
//! 
//! - **Parallel Workers**: Number of concurrent upload threads
//! - **Read-Ahead Depth**: Pipeline depth for block prefetching
//! - **Batch Size**: Number of blocks processed in each upload batch
//! - **Force Reupload**: Option to override existing data in cloud storage
//! 
//! ## Usage Patterns
//! 
//! ```rust
//! // Configure upload parameters
//! let config = ConfirmedBlockUploadConfig {
//!     num_blocks_to_upload_in_parallel: 8,
//!     block_read_ahead_depth: 16,
//!     force_reupload: false,
//!     max_num_slots_to_check: 32,
//! };
//! 
//! // Upload block range to BigTable
//! let last_slot = upload_confirmed_blocks(
//!     blockstore, bigtable, starting_slot, ending_slot, config, exit
//! ).await?;
//! ```

use {
    crate::blockstore::Blockstore,
    crossbeam_channel::{bounded, unbounded},
    log::*,
    solana_clock::Slot,
    solana_measure::measure::Measure,
    std::{
        cmp::{max, min},
        collections::HashSet,
        result::Result,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    },
};

/// Configuration for confirmed block upload operations to BigTable
/// 
/// Defines parameters for optimizing upload performance, controlling resource usage,
/// and managing the upload pipeline behavior. All settings can be tuned based on
/// system capabilities and network conditions.
#[derive(Clone)]
pub struct ConfirmedBlockUploadConfig {
    /// Force re-upload of blocks that already exist in BigTable storage
    pub force_reupload: bool,
    /// Maximum number of slot positions to scan during upload operations
    pub max_num_slots_to_check: usize,
    /// Number of blocks to upload concurrently for parallel processing
    pub num_blocks_to_upload_in_parallel: usize,
    /// Read-ahead pipeline depth for block prefetching (must be >= parallel uploads)
    pub block_read_ahead_depth: usize,
}

impl Default for ConfirmedBlockUploadConfig {
    fn default() -> Self {
        let num_blocks_to_upload_in_parallel = num_cpus::get() / 2;
        ConfirmedBlockUploadConfig {
            force_reupload: false,
            max_num_slots_to_check: num_blocks_to_upload_in_parallel * 4,
            num_blocks_to_upload_in_parallel,
            block_read_ahead_depth: num_blocks_to_upload_in_parallel * 2,
        }
    }
}

/// Performance statistics for blockstore read operations during upload
/// 
/// Tracks metrics for monitoring and optimizing the block reading pipeline
/// that feeds data to the upload workers.
struct BlockstoreLoadStats {
    /// Total number of blocks successfully read from blockstore
    pub num_blocks_read: usize,
    /// Total time elapsed during block reading operations
    pub elapsed: Duration,
}

/// Upload a range of confirmed blocks from blockstore to BigTable cloud storage
/// 
/// Efficiently transfers blockchain data to Google Cloud BigTable for long-term
/// archival, backup, and analytics. The function implements optimized batch
/// processing with configurable parallelism and intelligent deduplication.
/// 
/// # Arguments
/// * `blockstore` - Source blockstore containing blockchain data to upload
/// * `bigtable` - Destination BigTable storage client for cloud archival
/// * `starting_slot` - First slot in the range to upload (inclusive)
/// * `ending_slot` - Last slot in the range to upload (inclusive) 
/// * `config` - Upload configuration parameters for performance tuning
/// * `exit` - Atomic flag for graceful cancellation of upload operations
/// 
/// # Returns
/// The slot number of the last block successfully processed. If no blocks
/// are found in the specified range, returns `ending_slot`.
/// 
/// # Performance
/// - Uses parallel workers for concurrent uploads
/// - Implements read-ahead pipeline for optimal I/O
/// - Avoids redundant uploads through existence checking
/// - Provides progress monitoring and error recovery
pub async fn upload_confirmed_blocks(
    blockstore: Arc<Blockstore>,
    bigtable: solana_storage_bigtable::LedgerStorage,
    starting_slot: Slot,
    ending_slot: Slot,
    config: ConfirmedBlockUploadConfig,
    exit: Arc<AtomicBool>,
) -> Result<Slot, Box<dyn std::error::Error>> {
    let mut measure = Measure::start("entire upload");

    info!(
        "Loading ledger slots from {} to {}",
        starting_slot, ending_slot
    );
    let blockstore_slots: Vec<_> = blockstore
        .rooted_slot_iterator(starting_slot)
        .map_err(|err| {
            format!("Failed to load entries starting from slot {starting_slot}: {err:?}")
        })?
        .take_while(|slot| *slot <= ending_slot)
        .collect();

    if blockstore_slots.is_empty() {
        warn!("Ledger has no slots from {starting_slot} to {ending_slot:?}");
        return Ok(ending_slot);
    }

    let first_blockstore_slot = *blockstore_slots.first().unwrap();
    let last_blockstore_slot = *blockstore_slots.last().unwrap();
    info!(
        "Found {} slots in the range ({}, {})",
        blockstore_slots.len(),
        first_blockstore_slot,
        last_blockstore_slot,
    );

    // Gather the blocks that are already present in bigtable, by slot
    let bigtable_slots = if !config.force_reupload {
        let mut bigtable_slots = vec![];
        info!(
            "Loading list of bigtable blocks between slots {} and {}...",
            first_blockstore_slot, last_blockstore_slot
        );

        let mut start_slot = first_blockstore_slot;
        while start_slot <= last_blockstore_slot {
            let mut next_bigtable_slots = loop {
                let num_bigtable_blocks = min(1000, config.max_num_slots_to_check * 2);
                match bigtable
                    .get_confirmed_blocks(start_slot, num_bigtable_blocks)
                    .await
                {
                    Ok(slots) => break slots,
                    Err(err) => {
                        error!("get_confirmed_blocks for {} failed: {:?}", start_slot, err);
                        // Consider exponential backoff...
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            };
            if next_bigtable_slots.is_empty() {
                break;
            }
            bigtable_slots.append(&mut next_bigtable_slots);
            start_slot = bigtable_slots.last().unwrap() + 1;
        }
        bigtable_slots
            .into_iter()
            .filter(|slot| *slot <= last_blockstore_slot)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // The blocks that still need to be uploaded is the difference between what's already in the
    // bigtable and what's in blockstore...
    let blocks_to_upload = {
        let blockstore_slots = blockstore_slots.into_iter().collect::<HashSet<_>>();
        let bigtable_slots = bigtable_slots.into_iter().collect::<HashSet<_>>();

        let mut blocks_to_upload = blockstore_slots
            .difference(&bigtable_slots)
            .cloned()
            .collect::<Vec<_>>();
        blocks_to_upload.sort_unstable();
        blocks_to_upload.truncate(config.max_num_slots_to_check);
        blocks_to_upload
    };

    if blocks_to_upload.is_empty() {
        info!(
            "No blocks between {} and {} need to be uploaded to bigtable",
            starting_slot, ending_slot
        );
        return Ok(ending_slot);
    }
    let last_slot = *blocks_to_upload.last().unwrap();
    info!(
        "{} blocks to be uploaded to the bucket in the range ({}, {})",
        blocks_to_upload.len(),
        blocks_to_upload.first().unwrap(),
        last_slot
    );

    // Distribute the blockstore reading across a few background threads to speed up the bigtable uploading
    let (loader_threads, receiver): (Vec<_>, _) = {
        let exit = exit.clone();

        let (sender, receiver) = bounded(config.block_read_ahead_depth);

        let (slot_sender, slot_receiver) = unbounded();
        blocks_to_upload
            .into_iter()
            .for_each(|b| slot_sender.send(b).unwrap());
        drop(slot_sender);

        (
            (0..config.num_blocks_to_upload_in_parallel)
                .map(|i| {
                    let blockstore = blockstore.clone();
                    let sender = sender.clone();
                    let slot_receiver = slot_receiver.clone();
                    let exit = exit.clone();
                    std::thread::Builder::new()
                        .name(format!("solBigTGetBlk{i:02}"))
                        .spawn(move || {
                            let start = Instant::now();
                            let mut num_blocks_read = 0;

                            while let Ok(slot) = slot_receiver.recv() {
                                if exit.load(Ordering::Relaxed) {
                                    break;
                                }

                                let _ = match blockstore.get_rooted_block_with_entries(slot, true) {
                                    Ok(confirmed_block_with_entries) => {
                                        num_blocks_read += 1;
                                        sender.send((slot, Some(confirmed_block_with_entries)))
                                    }
                                    Err(err) => {
                                        warn!(
                                            "Failed to get load confirmed block from slot {}: {:?}",
                                            slot, err
                                        );
                                        sender.send((slot, None))
                                    }
                                };
                            }
                            BlockstoreLoadStats {
                                num_blocks_read,
                                elapsed: start.elapsed(),
                            }
                        })
                        .unwrap()
                })
                .collect(),
            receiver,
        )
    };

    let mut failures = 0;
    use futures::stream::StreamExt;

    let mut stream =
        tokio_stream::iter(receiver.into_iter()).chunks(config.num_blocks_to_upload_in_parallel);

    while let Some(blocks) = stream.next().await {
        if exit.load(Ordering::Relaxed) {
            break;
        }

        let mut measure_upload = Measure::start("Upload");
        let mut num_blocks = blocks.len();
        info!("Preparing the next {} blocks for upload", num_blocks);

        let uploads = blocks.into_iter().filter_map(|(slot, block)| match block {
            None => {
                num_blocks -= 1;
                None
            }
            Some(confirmed_block) => {
                let bt = bigtable.clone();
                Some(tokio::spawn(async move {
                    bt.upload_confirmed_block_with_entries(slot, confirmed_block)
                        .await
                }))
            }
        });

        for result in futures::future::join_all(uploads).await {
            if let Err(err) = result {
                error!("upload_confirmed_block() join failed: {:?}", err);
                failures += 1;
            } else if let Err(err) = result.unwrap() {
                error!("upload_confirmed_block() upload failed: {:?}", err);
                failures += 1;
            }
        }

        measure_upload.stop();
        info!("{} for {} blocks", measure_upload, num_blocks);
    }

    measure.stop();
    info!("{}", measure);

    let blockstore_results = loader_threads.into_iter().map(|t| t.join());

    let mut blockstore_num_blocks_read = 0;
    let mut blockstore_load_wallclock = Duration::default();
    let mut blockstore_errors = 0;

    for r in blockstore_results {
        match r {
            Ok(stats) => {
                blockstore_num_blocks_read += stats.num_blocks_read;
                blockstore_load_wallclock = max(stats.elapsed, blockstore_load_wallclock);
            }
            Err(e) => {
                error!("error joining blockstore thread: {:?}", e);
                blockstore_errors += 1;
            }
        }
    }

    info!(
        "blockstore upload took {:?} for {} blocks ({:.2} blocks/s) errors: {}",
        blockstore_load_wallclock,
        blockstore_num_blocks_read,
        blockstore_num_blocks_read as f64 / blockstore_load_wallclock.as_secs_f64(),
        blockstore_errors
    );

    if failures > 0 {
        Err(format!("Incomplete upload, {failures} operations failed").into())
    } else {
        Ok(last_slot)
    }
}
