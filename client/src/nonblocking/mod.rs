//! # Nonblocking Client Module
//!
//! This module provides asynchronous (async/await) versions of all client components.
//! It's designed for applications that need high concurrency and non-blocking I/O
//! operations when interacting with the Solana blockchain.
//!
//! ## Key Features
//!
//! - **Async/Await Support**: Full async/await compatibility for modern Rust applications
//! - **High Concurrency**: Efficient handling of thousands of concurrent operations
//! - **Non-blocking I/O**: All network operations are non-blocking
//! - **Tokio Integration**: Built on top of the Tokio async runtime
//!
//! ## Components
//!
//! - **TPU Client**: Async TPU communication for high-performance transaction submission
//! - **RPC Client**: Async RPC operations for blockchain queries
//! - **PubSub Client**: Real-time blockchain event subscriptions
//! - **Utility Modules**: Async versions of blockhash queries and nonce utilities
//!
//! ## Usage Pattern
//!
//! ```rust
//! use solana_client::nonblocking::{rpc_client::RpcClient, tpu_client::TpuClient};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let rpc_client = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
//!     let balance = rpc_client.get_balance(&pubkey).await?;
//!     Ok(())
//! }
//! ```

/// Async TPU client for high-performance transaction submission
pub mod tpu_client;

/// Async blockhash query utilities for transaction preparation
pub mod blockhash_query {
    pub use solana_rpc_client_nonce_utils::nonblocking::blockhash_query::*;
}

/// Async durable transaction nonce helpers for reliable transaction processing
pub mod nonce_utils {
    pub use solana_rpc_client_nonce_utils::nonblocking::*;
}

/// Async publish-subscribe client for real-time blockchain event streaming
pub mod pubsub_client {
    pub use solana_pubsub_client::nonblocking::pubsub_client::*;
}

/// Async communication with Solana nodes over RPC.
///
/// This module provides asynchronous versions of all RPC operations, enabling
/// high-concurrency applications to efficiently query blockchain state and
/// submit transactions without blocking execution.
///
/// The async RPC client is built on top of the Tokio runtime and provides
/// the same functionality as the blocking client but with async/await syntax.
///
/// # Example
/// ```rust
/// use solana_client::nonblocking::rpc_client::RpcClient;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
///     let slot = client.get_slot().await?;
///     println!("Current slot: {}", slot);
///     Ok(())
/// }
/// ```
///
/// [JSON-RPC]: https://www.jsonrpc.org/specification
/// [`RpcClient`]: crate::nonblocking::rpc_client::RpcClient
pub mod rpc_client {
    pub use solana_rpc_client::nonblocking::rpc_client::*;
}
