//! # Agave Transaction View Library
//!
//! This library provides a high-performance, zero-copy approach to parsing and accessing
//! Solana transaction data without full deserialization. It's designed for scenarios where
//! you need to efficiently extract specific fields from serialized transactions without
//! the overhead of deserializing the entire transaction structure.
//!
//! ## Key Features
//!
//! - **Zero-Copy Parsing**: Direct access to transaction fields from serialized bytes
//! - **Memory Efficient**: Minimal memory allocation during parsing
//! - **Performance Optimized**: Faster than traditional deserialization for selective access
//! - **Safety Focused**: Comprehensive bounds checking and validation
//! - **Version Agnostic**: Supports both legacy and versioned transaction formats
//!
//! ## Core Concepts
//!
//! ### TransactionView
//! The main interface for accessing transaction data. Provides methods to extract:
//! - Signatures and signature metadata
//! - Account keys (both static and lookup-resolved)
//! - Instructions with program IDs and account indices
//! - Address table lookups for account resolution
//! - Transaction metadata (version, blockhash, etc.)
//!
//! ### Frame-Based Parsing
//! Transactions are parsed into "frames" that contain metadata about where specific
//! data structures are located within the serialized bytes. This allows for:
//! - Lazy evaluation of transaction components
//! - Efficient iteration over variable-length structures
//! - Minimal memory overhead for metadata storage
//!
//! ### Sanitization
//! The library includes comprehensive validation to ensure transaction safety:
//! - Signature count validation
//! - Account access permission checks
//! - Instruction bounds validation
//! - Address table lookup consistency verification
//!
//! ## Usage Example
//!
//! ```rust
//! use agave_transaction_view::{
//!     transaction_view::TransactionView,
//!     transaction_data::TransactionData,
//! };
//!
//! // Parse a serialized transaction
//! let serialized_tx: &[u8] = /* transaction bytes */;
//! let view = TransactionView::try_new_sanitized(serialized_tx)?;
//!
//! // Access transaction components
//! let signatures = view.signatures();
//! let account_keys = view.static_account_keys();
//! let recent_blockhash = view.recent_blockhash();
//!
//! // Iterate over instructions
//! for instruction in view.instructions_iter() {
//!     println!("Program ID index: {}", instruction.program_id_index);
//!     println!("Accounts: {:?}", instruction.accounts);
//!     println!("Data: {:?}", instruction.data);
//! }
//! ```

// Parsing helpers are conditionally exposed for benchmarks and testing
#[cfg(feature = "dev-context-only-utils")]
pub mod bytes;  // Public for benchmark access
#[cfg(not(feature = "dev-context-only-utils"))]
mod bytes;      // Private for normal usage

// Internal frame parsing modules - these handle the low-level byte parsing
mod address_table_lookup_frame;  // Parses address table lookup structures
mod instructions_frame;          // Parses instruction data and provides iteration
mod message_header_frame;        // Parses transaction message headers
mod sanitize;                    // Transaction validation and safety checks
mod signature_frame;             // Parses signature data and metadata
mod transaction_frame;           // Main transaction structure parser

// Public API modules - these provide the main interfaces for users
pub mod resolved_transaction_view;  // Transaction view with resolved address lookups
pub mod result;                     // Error types and result handling
pub mod static_account_keys_frame;  // Static account key parsing utilities
pub mod transaction_data;           // Trait for abstracting transaction data sources
pub mod transaction_version;        // Transaction version enumeration
pub mod transaction_view;          // Main transaction view interface
