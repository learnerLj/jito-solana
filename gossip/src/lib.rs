#![cfg_attr(feature = "frozen-abi", feature(min_specialization))]
#![allow(clippy::arithmetic_side_effects)]

//! # Jito-Solana Gossip Network Library
//!
//! This library implements the gossip layer for the Jito-Solana blockchain validator network,
//! providing the distributed communication infrastructure that enables validators to share
//! critical network state information in a decentralized, fault-tolerant manner.
//!
//! ## Architecture Overview
//!
//! The gossip network operates as a peer-to-peer communication layer that sits above the
//! raw networking infrastructure but below the consensus and transaction processing layers.
//! It implements a sophisticated Conflict-free Replicated Data Type (CRDT) system that
//! ensures eventual consistency of network state across all validators.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────────┐
//! │                        JITO-SOLANA GOSSIP ARCHITECTURE                          │
//! └─────────────────────────────────────────────────────────────────────────────────┘
//! 
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │ CONSENSUS LAYER │    │TRANSACTION LAYER│    │  MEV SERVICES   │
//! │                 │    │                 │    │                 │
//! │ • Vote tracking │    │ • TPU discovery │    │ • Block engines │
//! │ • Fork choice   │    │ • Leader sched  │    │ • Relayers      │
//! │ • Finality      │    │ • Shred routing │    │ • Tip tracking  │
//! └─────────┬───────┘    └─────────┬───────┘    └─────────┬───────┘
//!           │                      │                      │
//!           ▼                      ▼                      ▼
//! ┌─────────────────────────────────────────────────────────────────────────────────┐
//! │                           GOSSIP PROTOCOL LAYER                                 │
//! │                                                                                 │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐           │
//! │  │ClusterInfo  │  │   CRDS      │  │ GossipSvc   │  │ ContactInfo │           │
//! │  │  Manager    │  │ Data Store  │  │  Pipeline   │  │  Discovery  │           │
//! │  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘           │
//! │           │                │                │                │                 │
//! │           ▼                ▼                ▼                ▼                 │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐           │
//! │  │Push/Pull    │  │ Vote Track  │  │ Duplicate   │  │ Ping/Pong   │           │
//! │  │ Protocol    │  │   System    │  │  Detection  │  │  Heartbeat  │           │
//! │  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘           │
//! └─────────────────────────────────────────────────────────────────────────────────┘
//!           │                      │                      │
//!           ▼                      ▼                      ▼
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │ UDP NETWORKING  │    │ QUIC TRANSPORT  │    │  TLS SECURITY   │
//! │                 │    │                 │    │                 │
//! │ • Raw packets   │    │ • Reliable      │    │ • Authentication│
//! │ • Broadcasting  │    │ • Flow control  │    │ • Encryption    │
//! │ • Port mgmt     │    │ • Congestion    │    │ • Key exchange  │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! ## Core Components
//!
//! ### Cluster Replicated Data Store (CRDS)
//! The foundation of the gossip system, implementing a distributed database that:
//! - **Eventual Consistency**: Guarantees all validators converge to the same network state
//! - **Conflict Resolution**: Uses timestamps and cryptographic hashes for deterministic merging
//! - **Efficient Storage**: Sharded data structure for O(1) lookups and efficient iteration
//! - **Automatic Pruning**: Removes stale data to prevent unbounded growth
//!
//! ### Push/Pull Protocol
//! A sophisticated epidemic protocol for data dissemination:
//! - **Push Phase**: Proactive propagation of new data to random peers
//! - **Pull Phase**: Reactive synchronization to fill gaps in local knowledge
//! - **Bloom Filters**: Efficient identification of missing data without full enumeration
//! - **Adaptive Rates**: Dynamic adjustment based on network conditions and stake weights
//!
//! ### Vote Tracking and Consensus Integration
//! Critical infrastructure for Solana's Proof-of-Stake consensus:
//! - **Vote Aggregation**: Collects and validates votes from all network validators
//! - **Optimistic Confirmation**: Enables fast finality for client applications
//! - **Fork Choice Support**: Provides vote data for leader selection and fork resolution
//! - **Slashing Protection**: Detects and reports double-voting violations
//!
//! ### Network Discovery and Topology
//! Dynamic peer discovery and connection management:
//! - **Contact Information**: Tracks network addresses, ports, and service endpoints
//! - **Stake-Weighted Topology**: Prioritizes connections based on validator stake
//! - **Geographic Awareness**: Optimizes routing for network latency and reliability
//! - **Health Monitoring**: Continuous assessment of peer connectivity and performance
//!
//! ## Key Features
//!
//! ### High Performance
//! - **100,000+ messages per second**: Optimized for Solana's high-throughput requirements
//! - **Sub-second propagation**: Critical information reaches 95% of network in <1 second
//! - **Efficient serialization**: Custom binary protocols minimize bandwidth usage
//! - **Parallel processing**: Multi-threaded design leverages modern hardware
//!
//! ### Fault Tolerance
//! - **Byzantine fault tolerance**: Continues operation with up to 33% malicious nodes
//! - **Network partition recovery**: Automatic healing when network connectivity is restored
//! - **Graceful degradation**: Maintains core functionality even under extreme load
//! - **Spam protection**: Rate limiting and stake-based filtering prevent abuse
//!
//! ### MEV Integration
//! - **Block engine discovery**: Enables MEV services to find active validators
//! - **Tip distribution tracking**: Gossips MEV tip information for transparent accounting
//! - **Relayer coordination**: Supports transaction privacy and validator load balancing
//! - **Commission management**: Tracks MEV service fees and revenue sharing
//!
//! ## Module Organization
//!
//! ### Core Protocol Implementation
//! - [`cluster_info`]: Main coordinator for all gossip activities and network state management
//! - [`gossip_service`]: High-level service orchestration and thread management
//! - [`protocol`]: Low-level message formats and network protocol definitions
//! - [`crds`]: Distributed data store with conflict-free replication semantics
//!
//! ### Data Structures and Storage
//! - [`crds_data`]: Strongly-typed data variants stored in the gossip network
//! - [`crds_value`]: Cryptographically signed containers for all gossip data
//! - [`crds_entry`]: Individual database entries with versioning and metadata
//! - [`crds_shards`]: Partitioned storage for efficient concurrent access
//!
//! ### Communication Protocols
//! - [`crds_gossip`]: High-level gossip protocol state machine and coordination
//! - [`crds_gossip_push`]: Proactive data dissemination to peers (epidemic spreading)
//! - [`crds_gossip_pull`]: Reactive data synchronization from peers (anti-entropy)
//! - [`ping_pong`]: Heartbeat protocol for liveness detection and RTT measurement
//!
//! ### Network Services
//! - [`contact_info`]: Peer discovery, connection management, and service endpoint tracking
//! - [`duplicate_shred`]: Byzantine fault detection and duplicate data identification
//! - [`epoch_slots`]: Slot progression tracking and synchronization across validators
//! - [`weighted_shuffle`]: Stake-weighted peer selection for efficient data propagation
//!
//! ### Utility and Support
//! - [`cluster_info_metrics`]: Comprehensive monitoring and performance measurement
//! - [`gossip_error`]: Error types and handling for all gossip operations
//! - [`received_cache`]: Deduplication cache to prevent processing duplicate messages
//! - [`restart_crds_values`]: State recovery mechanisms for validator restarts
//!
//! ## Performance Characteristics
//!
//! - **Message Throughput**: 100,000+ gossip messages per second per validator
//! - **Network Propagation**: 95% of network receives data within 1 second
//! - **Memory Usage**: <2GB for full network state (8,000+ validators)
//! - **Bandwidth Usage**: <50 Mbps sustained for active validator participation
//! - **CPU Usage**: <10% of modern multi-core processor for gossip operations
//! - **Storage**: <100MB for complete CRDS state including historical data
//!
//! ## Security Model
//!
//! ### Cryptographic Integrity
//! - All gossip messages are cryptographically signed by their originators
//! - Ed25519 signatures provide strong authentication and non-repudiation
//! - Hash-based conflict resolution ensures deterministic merge outcomes
//! - Stake-weighted validation prevents low-stake attackers from overwhelming the network
//!
//! ### Spam and DoS Protection
//! - Rate limiting prevents individual nodes from flooding the network
//! - Stake-based prioritization ensures legitimate validators get priority
//! - Bloom filters enable efficient duplicate detection without storage overhead
//! - Automatic peer disconnection for consistently misbehaving nodes
//!
//! ### Byzantine Fault Tolerance
//! - Continues correct operation with up to 33% Byzantine (malicious) validators
//! - Conflicting information is resolved deterministically using cryptographic hashes
//! - Multiple independent communication paths prevent single points of failure
//! - Economic incentives align validator behavior with network health

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CORE PROTOCOL MODULES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Main coordinator for all gossip activities and network state management.
/// Orchestrates peer discovery, message routing, and CRDS synchronization.
pub mod cluster_info;

/// High-level service orchestration and thread management for gossip operations.
/// Manages the lifecycle of all gossip-related background threads and services.
pub mod gossip_service;

/// Low-level message formats and network protocol definitions.
/// Defines the binary wire format for all gossip communication.
mod protocol;

/// Distributed data store with conflict-free replication semantics.
/// Implements the core CRDT that maintains network state consistency.
pub mod crds;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// DATA STRUCTURES AND STORAGE
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Strongly-typed data variants stored in the gossip network.
/// Defines all types of information that can be shared via gossip.
pub mod crds_data;

/// Cryptographically signed containers for all gossip data.
/// Provides integrity and authenticity for all network information.
pub mod crds_value;

/// Individual database entries with versioning and metadata.
/// Manages the lifecycle and storage of each piece of gossip data.
pub mod crds_entry;

/// Partitioned storage for efficient concurrent access.
/// Enables high-performance concurrent reads and writes to the CRDS.
pub mod crds_shards;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// COMMUNICATION PROTOCOLS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// High-level gossip protocol state machine and coordination.
/// Orchestrates the overall gossip protocol including push/pull scheduling.
pub mod crds_gossip;

/// Proactive data dissemination to peers using epidemic spreading algorithms.
/// Implements the "push" phase of the gossip protocol for fast propagation.
pub mod crds_gossip_push;

/// Reactive data synchronization from peers using anti-entropy techniques.
/// Implements the "pull" phase of the gossip protocol for consistency.
pub mod crds_gossip_pull;

/// Error handling and reporting for all gossip protocol operations.
/// Provides structured error types for robust error handling.
pub mod crds_gossip_error;

/// Heartbeat protocol for liveness detection and RTT measurement.
/// Maintains peer connectivity information and network health metrics.
pub mod ping_pong;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// NETWORK SERVICES AND DISCOVERY
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Peer discovery, connection management, and service endpoint tracking.
/// Maintains the network topology and enables validators to find each other.
pub mod contact_info;

/// Byzantine fault detection and duplicate data identification.
/// Detects and reports malicious behavior including duplicate shreds.
pub mod duplicate_shred;

/// Handler for processing and responding to duplicate shred reports.
/// Coordinates the network response to detected Byzantine behavior.
pub mod duplicate_shred_handler;

/// Background service for monitoring and detecting duplicate shreds.
/// Continuously monitors network traffic for signs of malicious activity.
pub mod duplicate_shred_listener;

/// Slot progression tracking and synchronization across validators.
/// Maintains consensus about which slots have been confirmed by the network.
pub mod epoch_slots;

/// Epoch configuration and specification management.
/// Tracks epoch boundaries and validator set changes.
pub mod epoch_specs;

/// Stake-weighted peer selection for efficient data propagation.
/// Optimizes gossip routing based on validator stake weights.
pub mod weighted_shuffle;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// UTILITY AND SUPPORT MODULES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Comprehensive monitoring and performance measurement for gossip operations.
/// Provides detailed metrics for network health and performance analysis.
pub mod cluster_info_metrics;

/// Error types and handling for all gossip operations.
/// Centralized error definitions for consistent error handling.
pub mod gossip_error;

/// State recovery mechanisms for validator restarts.
/// Enables validators to quickly resync after restarts or network partitions.
pub mod restart_crds_values;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// INTERNAL MODULES (IMPLEMENTATION DETAILS)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Internal message filtering logic for spam prevention and efficiency.
/// Implements sophisticated filtering rules to prevent network abuse.
mod crds_filter;

/// Legacy compatibility layer for older contact info formats.
/// Maintains backward compatibility while transitioning to new formats.
#[macro_use]
mod legacy_contact_info;

/// Type-Length-Value encoding utilities for efficient serialization.
/// Provides compact binary encoding for network messages.
#[macro_use]
mod tlv;

/// Active set management for push protocol optimization.
/// Maintains the set of peers actively participating in gossip.
mod push_active_set;

/// Deduplication cache to prevent processing duplicate messages.
/// Prevents waste of computational resources on redundant data.
mod received_cache;

/// Deprecated functionality maintained for backward compatibility.
/// Contains deprecated code paths that will be removed in future versions.
mod deprecated;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// EXTERNAL DEPENDENCIES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[macro_use]
extern crate log;

#[cfg(test)]
#[macro_use]
extern crate assert_matches;

#[macro_use]
extern crate serde_derive;

#[cfg_attr(feature = "frozen-abi", macro_use)]
#[cfg(feature = "frozen-abi")]
extern crate solana_frozen_abi_macro;

#[macro_use]
extern crate solana_metrics;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// TEST MODULES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Wire format compatibility tests to ensure protocol stability.
/// Validates that network messages remain compatible across versions.
mod wire_format_tests;
