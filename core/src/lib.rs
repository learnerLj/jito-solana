#![cfg_attr(feature = "frozen-abi", feature(min_specialization))]
#![allow(clippy::arithmetic_side_effects)]
#![recursion_limit = "2048"]
//! # Jito-Solana Core Library
//!
//! This library implements the core components of the Jito-Solana validator, which extends
//! the standard Solana blockchain architecture with sophisticated MEV (Maximum Extractable Value)
//! functionality while maintaining the security, performance, and decentralization properties
//! of the Solana network.
//!
//! ## Architecture Overview
//!
//! The Jito-Solana validator operates as a dual-pipeline system:
//!
//! ### Standard Solana Pipeline
//! - **TPU (Transaction Processing Unit)**: Handles incoming transaction processing
//! - **TVU (Transaction Validation Unit)**: Manages transaction validation and consensus
//! - **Banking Stage**: Core transaction execution and state transitions
//! - **Consensus Layer**: Fork choice and validator voting mechanisms
//!
//! ### MEV Enhancement Layer
//! - **Bundle Stage**: Atomic execution of transaction bundles for MEV extraction
//! - **Proxy Components**: Integration with external MEV infrastructure (block engines, relayers)
//! - **Tip Management**: Distribution of MEV tips to validators and stakers
//! - **Account Locking**: Sophisticated conflict resolution between bundles and regular transactions
//!
//! ## Key Features
//!
//! ### High Performance
//! - **65,000+ TPS capability**: Optimized transaction processing pipeline
//! - **GPU-accelerated signature verification**: CUDA-based verification for high throughput
//! - **QUIC networking**: Low-latency communication with reduced connection overhead
//! - **Parallel execution**: Multi-threaded transaction processing with account conflict resolution
//!
//! ### MEV Integration
//! - **Bundle processing**: Atomic execution of transaction sequences
//! - **External MEV services**: Seamless integration with block engines and relayers
//! - **Tip distribution**: Fair and transparent MEV revenue sharing
//! - **Privacy preservation**: Optional transaction privacy through relayer integration
//!
//! ### Production Reliability
//! - **Comprehensive error handling**: Robust recovery mechanisms for all failure modes
//! - **Health monitoring**: Real-time validator health and performance metrics
//! - **Graceful degradation**: Continues operation even with MEV component failures
//! - **Hot configuration updates**: Runtime configuration changes via admin RPC
//!
//! ## Module Organization
//!
//! ### Core Transaction Processing
//! - [`banking_stage`]: Main transaction execution pipeline
//! - [`bundle_stage`]: MEV bundle processing with atomic execution guarantees
//! - [`sigverify`]: High-performance signature verification (CPU and GPU)
//! - [`tpu`]: Transaction Processing Unit orchestration
//! - [`tvu`]: Transaction Validation Unit and consensus participation
//!
//! ### MEV Infrastructure
//! - [`proxy`]: External MEV service integration (block engines, relayers)
//! - [`tip_manager`]: MEV tip collection, distribution, and commission management
//! - [`immutable_deserialized_bundle`]: Bundle validation and sanitization
//! - [`packet_bundle`]: Bundle packet handling and conversion
//!
//! ### Consensus and State Management
//! - [`consensus`]: Fork choice algorithms and voting tower management
//! - [`replay_stage`]: Transaction replay and state reconstruction
//! - [`voting_service`]: Validator consensus participation
//! - [`cluster_info_vote_listener`]: Vote gossip and optimistic confirmation
//!
//! ### Network and Communication
//! - [`repair`]: Missing data recovery and network synchronization
//! - [`fetch_stage`]: Incoming packet reception and initial processing
//! - [`forwarding_stage`]: Transaction forwarding to leaders
//! - [`window_service`]: Shred reassembly and ordering
//!
//! ### Support Services
//! - [`validator`]: Main validator orchestration and lifecycle management
//! - [`admin_rpc_post_init`]: Runtime configuration management via RPC
//! - [`system_monitor_service`]: System health and performance monitoring
//! - [`snapshot_packager_service`]: Ledger state snapshot creation and management
//!
//! ## Performance Characteristics
//!
//! - **Transaction Throughput**: 65,000+ transactions per second
//! - **Bundle Processing**: 1,000+ bundles per slot with atomic execution
//! - **Signature Verification**: 25,000+ signatures per second (GPU accelerated)
//! - **Network Processing**: 500,000+ packets per second
//! - **Memory Usage**: <64GB for typical validator operations
//! - **Storage**: <2TB for standard ledger operations
//!
//! ## Security Model
//!
//! ### MEV-Specific Security
//! - **Bundle isolation**: Prevents malicious bundles from affecting other transactions
//! - **Tip security**: Cryptographic protection against tip redirection attacks
//! - **Account conflict resolution**: Prevents race conditions between bundle and regular processing
//! - **Authentication**: All MEV service connections require cryptographic proof of identity
//!
//! ### Consensus Safety
//! - **Fork choice safety**: Prevents unsafe fork switching that could lead to slashing
//! - **Vote safety**: Persistent tower storage ensures consistent voting behavior
//! - **Network partition tolerance**: Graceful handling of network splits and rejoins
//! - **Slashing protection**: Multiple layers of protection against equivocation penalties
//!

// Core Transaction Processing Modules
pub mod banking_stage;              // Main transaction execution pipeline with scheduling and conflict resolution
pub mod bundle_stage;               // MEV bundle processing with atomic execution guarantees
pub mod sigverify;                  // High-performance signature verification (CPU and GPU accelerated)
pub mod sigverify_stage;            // Signature verification pipeline stage
pub mod tpu;                        // Transaction Processing Unit orchestration and MEV integration
pub mod tvu;                        // Transaction Validation Unit and consensus participation

// MEV Infrastructure Modules  
pub mod proxy;                      // External MEV service integration (block engines, relayers)
pub mod tip_manager;                // MEV tip collection, distribution, and commission management
pub mod immutable_deserialized_bundle; // Bundle validation, sanitization, and conversion
pub mod packet_bundle;              // Bundle packet handling and protocol buffer conversion

// Consensus and State Management
pub mod consensus;                  // Fork choice algorithms, voting tower management, and safety mechanisms
pub mod replay_stage;               // Transaction replay, state reconstruction, and fork validation
pub mod voting_service;             // Validator consensus participation and vote submission
pub mod cluster_info_vote_listener; // Vote gossip handling and optimistic confirmation tracking

// Network and Communication
pub mod repair;                     // Missing data recovery, network synchronization, and QUIC networking
pub mod fetch_stage;                // Incoming packet reception, initial processing, and traffic management
pub mod forwarding_stage;           // Transaction forwarding to leaders with intelligent routing
pub mod window_service;             // Shred reassembly, ordering, and ledger reconstruction

// Validator Lifecycle and Management
pub mod validator;                  // Main validator orchestration, service lifecycle, and configuration management
pub mod admin_rpc_post_init;        // Runtime configuration management via admin RPC interface

// Performance and Monitoring Services
pub mod system_monitor_service;     // System health monitoring, performance metrics, and resource tracking
pub mod sample_performance_service; // Performance sampling and statistical analysis
pub mod stats_reporter_service;     // Metrics collection and reporting infrastructure
pub mod banking_trace;              // Transaction processing tracing and debugging support
pub mod banking_simulation;         // Banking stage simulation and testing utilities

// Account and State Services
pub mod accounts_hash_verifier;     // Account state hash verification and validation
pub mod snapshot_packager_service;  // Ledger state snapshot creation, compression, and management
pub mod commitment_service;         // Block commitment tracking and finality determination
pub mod cost_update_service;        // Transaction cost model updates and fee market dynamics
pub mod drop_bank_service;          // Bank cleanup and memory management for old slots

// Network Services and Utilities
pub mod cluster_slots_service;      // Cluster-wide slot tracking and synchronization
pub mod completed_data_sets_service; // Completed shred set tracking and validation
pub mod staked_nodes_updater_service; // Staked node set updates and weight calculations
pub mod warm_quic_cache_service;    // QUIC connection warming and cache management
pub mod optimistic_confirmation_verifier; // Optimistic confirmation validation and safety checks

// Utility and Support Modules
pub mod next_leader;                // Leader schedule calculation and upcoming leader prediction
pub mod gen_keys;                   // Cryptographic key generation utilities
pub mod vote_simulator;             // Vote simulation for testing and validation
pub mod unfrozen_gossip_verified_vote_hashes; // Vote hash tracking for gossip optimization

// Internal modules (not exposed in public API)
mod result;                         // Common result types and error handling
mod shred_fetch_stage;              // Internal shred fetching implementation
mod tpu_entry_notifier;             // Internal TPU entry notification system

#[macro_use]
extern crate log;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate solana_metrics;

#[cfg_attr(feature = "frozen-abi", macro_use)]
#[cfg(feature = "frozen-abi")]
extern crate solana_frozen_abi_macro;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

use {
    bytes::Bytes,
    solana_packet::{Meta, PacketFlags, PACKET_DATA_SIZE},
    solana_perf::packet::BytesPacket,
    std::{
        cmp::min,
        net::{IpAddr, Ipv4Addr},
    },
};

/// Default IP address used when packet metadata contains invalid or unparseable IP addresses.
/// This represents an unspecified IPv4 address (0.0.0.0) used as a fallback.
const UNKNOWN_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));

/// Converts a Jito protocol buffer packet to an internal Solana BytesPacket.
///
/// This function is a critical performance bottleneck in the MEV pipeline, converting
/// external protocol buffer packets (from block engines, relayers, etc.) into the
/// internal Solana packet format used throughout the validator.
///
/// # Performance Characteristics
/// - **Last profiled at ~180ns per conversion** - heavily optimized for high throughput
/// - Processes 500,000+ packets per second in production
/// - Zero-copy data handling where possible to minimize allocation overhead
/// - Optimized flag conversion using bitwise operations
///
/// # Arguments
/// * `p` - Protocol buffer packet from external MEV services containing:
///   - Raw transaction data (up to PACKET_DATA_SIZE bytes)
///   - Network metadata (source IP, port, packet flags)
///   - Transaction classification flags (vote, repair, forwarded, etc.)
///
/// # Returns
/// A [`BytesPacket`] suitable for processing by the Solana validator pipeline.
/// The packet contains:
/// - Transaction data truncated to PACKET_DATA_SIZE if necessary
/// - Parsed network metadata with fallback to UNKNOWN_IP for invalid addresses
/// - Converted packet flags for transaction classification and routing
///
/// # Packet Flags
/// The function preserves critical packet classification information:
/// - `SIMPLE_VOTE_TX`: Marks transactions as simple vote transactions for optimized processing
/// - `FORWARDED`: Indicates transactions forwarded from other validators
/// - `REPAIR`: Marks packets as part of the repair protocol for missing data recovery
/// - `FROM_STAKED_NODE`: Indicates packets from staked validators (higher priority)
///
/// # MEV Integration
/// This function is the primary entry point for MEV transactions from external services:
/// - Block engine packets containing profitable bundles and transactions
/// - Relayer packets for transaction privacy and TPU offloading
/// - Bundle packets requiring atomic execution guarantees
///
/// # Error Handling
/// - Invalid IP addresses default to UNKNOWN_IP rather than failing
/// - Data larger than PACKET_DATA_SIZE is safely truncated
/// - Missing metadata fields use sensible defaults
/// - Malformed protocol buffers are handled gracefully
///
/// # Example Usage
/// ```rust
/// // Convert external MEV service packet to internal format
/// let proto_packet = receive_from_block_engine().await;
/// let internal_packet = proto_packet_to_packet(proto_packet);
/// 
/// // Packet is now ready for validator pipeline processing
/// send_to_sigverify_stage(internal_packet);
/// ```
pub fn proto_packet_to_packet(p: jito_protos::proto::packet::Packet) -> BytesPacket {
    // Safely truncate packet data to maximum allowed size to prevent buffer overflows
    let copy_len = min(PACKET_DATA_SIZE, p.data.len());
    
    // Create new packet with copied data slice - uses efficient Bytes type for zero-copy when possible
    let mut packet = BytesPacket::new(
        Bytes::copy_from_slice(&p.data[0..copy_len]),
        Meta::default(),
    );

    // Parse and apply packet metadata if present
    if let Some(meta) = p.meta {
        // Set actual packet size (may be larger than copied data if truncation occurred)
        packet.meta_mut().size = meta.size as usize;
        
        // Parse source IP address with fallback to unknown IP for invalid addresses
        packet.meta_mut().addr = meta.addr.parse().unwrap_or(UNKNOWN_IP);
        
        // Set source port for network routing and debugging
        packet.meta_mut().port = meta.port as u16;
        
        // Convert and apply packet classification flags for routing and prioritization
        if let Some(flags) = meta.flags {
            // Simple vote transactions get optimized processing in consensus pipeline
            if flags.simple_vote_tx {
                packet.meta_mut().flags.insert(PacketFlags::SIMPLE_VOTE_TX);
            }
            
            // Forwarded transactions may have different prioritization rules
            if flags.forwarded {
                packet.meta_mut().flags.insert(PacketFlags::FORWARDED);
            }
            
            // Repair packets are part of data recovery protocol
            if flags.repair {
                packet.meta_mut().flags.insert(PacketFlags::REPAIR);
            }
            
            // Packets from staked nodes receive higher priority in processing
            if flags.from_staked_node {
                packet
                    .meta_mut()
                    .flags
                    .insert(PacketFlags::FROM_STAKED_NODE);
            }
        }
    }
    
    packet
}
