//! Slot Statistics - Performance Monitoring and Analysis for Blockchain Slots
//! 
//! This module provides comprehensive statistics collection and analysis for individual
//! blockchain slots, tracking shred reception patterns, recovery operations, and slot
//! completion metrics. The statistics are essential for monitoring validator performance,
//! diagnosing network issues, and optimizing shred distribution strategies.
//! 
//! ## Core Functionality
//! 
//! - **Shred Source Tracking**: Monitors how shreds arrive (turbine, repair, recovery)
//! - **FEC Set Analysis**: Tracks forward error correction performance across sets
//! - **Completion Metrics**: Measures slot completion patterns and timing
//! - **Performance Monitoring**: Provides detailed insights into slot processing efficiency
//! - **Cache Management**: LRU-based caching for recent slot statistics
//! 
//! ## Statistics Categories
//! 
//! - **Turbine Reception**: Shreds received through normal gossip turbine protocol
//! - **Repair Operations**: Shreds obtained through targeted repair requests
//! - **Recovery Success**: Shreds reconstructed using Reed-Solomon erasure coding
//! - **Slot State Flags**: Dead, full, and rooted status tracking
//! 
//! ## Performance Insights
//! 
//! The statistics help identify:
//! - Network connectivity issues affecting shred reception
//! - Validator performance problems in slot processing
//! - Erasure coding effectiveness for data recovery
//! - Optimal turbine tree positioning for improved reception
//! 
//! ## Usage Patterns
//! 
//! ```rust
//! // Track shred reception
//! slots_stats.record_shred(slot, ShredSource::Turbine, false);
//! 
//! // Mark slot completion
//! slots_stats.mark_full(slot, last_index);
//! 
//! // Generate performance report
//! slots_stats.report_slot(slot);
//! ```

use {
    crate::blockstore_meta::SlotMeta,
    bitflags::bitflags,
    lru::LruCache,
    solana_clock::Slot,
    std::{
        collections::HashMap,
        sync::{Mutex, MutexGuard},
    },
};

/// Maximum number of slot statistics to maintain in the LRU cache
/// 
/// This capacity balances memory usage with the need to track recent slot
/// performance. The cache covers approximately 2-3 minutes of slots at
/// normal network speeds, providing adequate history for analysis.
const SLOTS_STATS_CACHE_CAPACITY: usize = 300;

/// Source classification for received shreds
/// 
/// Tracks how each shred was obtained to analyze network performance
/// and validator connectivity patterns. This information is crucial
/// for optimizing shred distribution and identifying network issues.
#[derive(Copy, Clone, Debug)]
pub(crate) enum ShredSource {
    /// Shred received through normal gossip turbine protocol
    Turbine,
    /// Shred obtained through targeted repair request to specific validator
    Repaired,
    /// Shred reconstructed using Reed-Solomon erasure coding from other shreds
    Recovered,
}

bitflags! {
    /// Status flags tracking slot lifecycle state
    /// 
    /// Efficient bit-packed representation of slot status for memory
    /// optimization and fast status queries during statistics collection.
    #[derive(Copy, Clone, Default)]
    struct SlotFlags: u8 {
        /// Slot marked as dead due to irrecoverable data loss or corruption
        const DEAD   = 0b00000001;
        /// Slot contains all required shreds and is complete
        const FULL   = 0b00000010;
        /// Slot has been finalized by network consensus (rooted)
        const ROOTED = 0b00000100;
    }
}

/// Comprehensive statistics for an individual blockchain slot
/// 
/// Collects detailed metrics about shred reception, error correction performance,
/// and slot completion patterns. These statistics enable performance monitoring,
/// network analysis, and optimization of validator operations.
#[derive(Clone, Default)]
pub struct SlotStats {
    /// Count of shreds received per FEC set through turbine protocol
    turbine_fec_set_index_counts: HashMap</*fec_set_index*/ u32, /*count*/ usize>,
    /// Total number of shreds obtained through repair operations
    num_repaired: usize,
    /// Total number of shreds recovered using erasure coding
    num_recovered: usize,
    /// Highest shred index observed for this slot
    last_index: u64,
    /// Current slot lifecycle status flags
    flags: SlotFlags,
}

impl SlotStats {
    pub fn get_min_index_count(&self) -> usize {
        self.turbine_fec_set_index_counts
            .values()
            .min()
            .copied()
            .unwrap_or_default()
    }

    fn report(&self, slot: Slot) {
        let min_fec_set_count = self.get_min_index_count();
        datapoint_info!(
            "slot_stats_tracking_complete",
            ("slot", slot, i64),
            ("last_index", self.last_index, i64),
            ("num_repaired", self.num_repaired, i64),
            ("num_recovered", self.num_recovered, i64),
            ("min_turbine_fec_set_count", min_fec_set_count, i64),
            ("is_full", self.flags.contains(SlotFlags::FULL), bool),
            ("is_rooted", self.flags.contains(SlotFlags::ROOTED), bool),
            ("is_dead", self.flags.contains(SlotFlags::DEAD), bool),
        );
    }
}

pub struct SlotsStats {
    pub stats: Mutex<LruCache<Slot, SlotStats>>,
}

impl Default for SlotsStats {
    fn default() -> Self {
        Self {
            stats: Mutex::new(LruCache::new(SLOTS_STATS_CACHE_CAPACITY)),
        }
    }
}

impl SlotsStats {
    fn get_or_default_with_eviction_check<'a>(
        stats: &'a mut MutexGuard<LruCache<Slot, SlotStats>>,
        slot: Slot,
    ) -> (&'a mut SlotStats, Option<(Slot, SlotStats)>) {
        let evicted = if stats.len() == stats.cap() {
            match stats.peek_lru() {
                Some((s, _)) if *s == slot => None,
                _ => stats.pop_lru(),
            }
        } else {
            None
        };
        stats.get_or_insert(slot, SlotStats::default);
        (stats.get_mut(&slot).unwrap(), evicted)
    }

    pub(crate) fn record_shred(
        &self,
        slot: Slot,
        fec_set_index: u32,
        source: ShredSource,
        slot_meta: Option<&SlotMeta>,
    ) {
        let mut slot_full_reporting_info = None;
        let mut stats = self.stats.lock().unwrap();
        let (slot_stats, evicted) = Self::get_or_default_with_eviction_check(&mut stats, slot);
        match source {
            ShredSource::Recovered => slot_stats.num_recovered += 1,
            ShredSource::Repaired => slot_stats.num_repaired += 1,
            ShredSource::Turbine => {
                *slot_stats
                    .turbine_fec_set_index_counts
                    .entry(fec_set_index)
                    .or_default() += 1
            }
        }
        if let Some(meta) = slot_meta {
            if meta.is_full() {
                slot_stats.last_index = meta.last_index.unwrap_or_default();
                if !slot_stats.flags.contains(SlotFlags::FULL) {
                    slot_stats.flags |= SlotFlags::FULL;
                    slot_full_reporting_info =
                        Some((slot_stats.num_repaired, slot_stats.num_recovered));
                }
            }
        }
        drop(stats);
        if let Some((num_repaired, num_recovered)) = slot_full_reporting_info {
            let slot_meta = slot_meta.unwrap();
            let total_time_ms =
                solana_time_utils::timestamp().saturating_sub(slot_meta.first_shred_timestamp);
            let last_index = slot_meta
                .last_index
                .and_then(|ix| i64::try_from(ix).ok())
                .unwrap_or(-1);
            datapoint_info!(
                "shred_insert_is_full",
                ("slot", slot, i64),
                ("total_time_ms", total_time_ms, i64),
                ("last_index", last_index, i64),
                ("num_repaired", num_repaired, i64),
                ("num_recovered", num_recovered, i64),
            );
        }
        if let Some((evicted_slot, evicted_stats)) = evicted {
            evicted_stats.report(evicted_slot);
        }
    }

    fn add_flag(&self, slot: Slot, flag: SlotFlags) {
        let evicted = {
            let mut stats = self.stats.lock().unwrap();
            let (slot_stats, evicted) = Self::get_or_default_with_eviction_check(&mut stats, slot);
            slot_stats.flags |= flag;
            evicted
        };
        if let Some((evicted_slot, evicted_stats)) = evicted {
            evicted_stats.report(evicted_slot);
        }
    }

    pub fn mark_dead(&self, slot: Slot) {
        self.add_flag(slot, SlotFlags::DEAD);
    }

    pub fn mark_rooted(&self, slot: Slot) {
        self.add_flag(slot, SlotFlags::ROOTED);
    }
}
