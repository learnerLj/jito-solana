//! Solana Ledger - Blockchain Data Storage and Management
//! 
//! This crate provides the core ledger functionality for the Solana blockchain, implementing
//! persistent storage, data integrity verification, and efficient access patterns for all
//! blockchain data including transactions, blocks, accounts, and metadata.
//! 
//! ## Key Features
//! 
//! - **High-Performance Storage**: RocksDB-based persistent storage optimized for blockchain workloads
//! - **Data Integrity**: Reed-Solomon erasure coding and cryptographic verification for fault tolerance
//! - **Efficient Access**: Fast lookups by slot, signature, address with comprehensive indexing
//! - **Real-time Processing**: Event-driven architecture for consensus and validation integration
//! - **Storage Lifecycle**: Automated cleanup, archival, and snapshot management
//! 
//! ## Core Components
//! 
//! - **Blockstore**: Primary storage engine with RocksDB backend
//! - **Shred Management**: Data unit encoding/decoding with erasure coding
//! - **Leader Scheduling**: Validator rotation and block production coordination
//! - **Entry Processing**: Transaction batch validation and state transitions
//! - **Cloud Integration**: BigTable archival and long-term storage
//! 
//! ## Usage
//! 
//! ```rust
//! use solana_ledger::blockstore::Blockstore;
//! 
//! // Initialize blockstore for ledger operations
//! let blockstore = Blockstore::open(&ledger_path)?;
//! 
//! // Store blockchain data as shreds
//! blockstore.insert_shreds(shreds, &leader_schedule, false)?;
//! 
//! // Retrieve complete blocks for validation
//! let block = blockstore.get_rooted_block(slot, true)?;
//! ```

#![cfg_attr(feature = "frozen-abi", feature(min_specialization))]
#![allow(clippy::arithmetic_side_effects)]
#![recursion_limit = "2048"]

// =============================================================================
// Core Storage and Database Modules
// =============================================================================

/// Primary blockchain data storage engine using RocksDB
#[macro_use]
pub mod blockstore;

/// Low-level database abstraction and column family management
pub mod blockstore_db;

/// Metadata structures for slots, erasure sets, and data integrity
pub mod blockstore_meta;

/// Configuration options for blockstore behavior and performance tuning
pub mod blockstore_options;

/// Block validation and state transition processing engine
pub mod blockstore_processor;

// =============================================================================
// Shred Management and Data Integrity
// =============================================================================

/// Core shred types, encoding/decoding, and validation logic
pub mod shred;

/// Shred creation from entries with Reed-Solomon erasure coding
mod shredder;

/// Cryptographic signature verification for shred authenticity
pub mod sigverify_shreds;

// =============================================================================
// Leader Schedule and Consensus Support
// =============================================================================

/// Validator rotation and block production ordering logic
pub mod leader_schedule;

/// Efficient caching for consensus operations and leader lookups
pub mod leader_schedule_cache;

/// Utility functions for leader schedule calculations and management
pub mod leader_schedule_utils;

// =============================================================================
// Data Access and Iteration Utilities
// =============================================================================

/// Iterator for traversing blockchain ancestry relationships
pub mod ancestor_iterator;

/// Iterator for processing subsequent slots in order
pub mod next_slots_iterator;

/// Iterator for processing finalized (rooted) slots only
pub mod rooted_slot_iterator;

// =============================================================================
// Storage Lifecycle and Maintenance
// =============================================================================

/// Automated cleanup service for old blockchain data
pub mod blockstore_cleanup_service;

/// Startup optimization using compressed state snapshots
pub mod use_snapshot_archives_at_startup;

// =============================================================================
// Event Processing and Notifications
// =============================================================================

/// Interface definition for real-time ledger entry event notifications
pub mod entry_notifier_interface;

/// Service implementation for broadcasting ledger entry events
pub mod entry_notifier_service;

// =============================================================================
// Analytics and Monitoring
// =============================================================================

/// Performance and health monitoring metrics collection
pub mod blockstore_metrics;

/// Automated metric reporting service for observability
pub mod blockstore_metric_report_service;

/// Per-slot performance statistics and analysis
pub mod slot_stats;

// =============================================================================
// Cloud Integration and Archival
// =============================================================================

/// Google Cloud BigTable integration for long-term data archival
pub mod bigtable_upload;

/// BigTable upload service for automated cloud data synchronization
pub mod bigtable_upload_service;

/// Cloud data lifecycle management and deletion policies
pub mod bigtable_delete;

// =============================================================================
// Blockchain Utilities and Processing
// =============================================================================

/// Fork management and resolution utilities for consensus
pub mod bank_forks_utils;

/// Transaction balance tracking and analysis utilities
pub mod transaction_balances;

/// Genesis block creation and validation utilities
pub mod genesis_utils;

/// Blockchain error types and error handling
pub mod block_error;

// =============================================================================
// Internal Utilities and Support Modules
// =============================================================================

/// Efficient bit vector operations for data structure optimization
pub mod bit_vec;

/// Staking-related utility functions (internal use)
mod staking_utils;

/// Address lookup table scanning for transaction processing (internal use)
mod transaction_address_lookup_table_scanner;

#[macro_use]
extern crate eager;

#[macro_use]
extern crate solana_metrics;

#[macro_use]
extern crate log;

#[cfg_attr(feature = "frozen-abi", macro_use)]
#[cfg(feature = "frozen-abi")]
extern crate solana_frozen_abi_macro;

#[doc(hidden)]
pub mod macro_reexports {
    pub use solana_accounts_db::hardened_unpack::MAX_GENESIS_ARCHIVE_UNPACKED_SIZE;
}

mod wire_format_tests;
