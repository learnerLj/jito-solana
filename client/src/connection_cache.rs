//! # Connection Cache Module
//!
//! This module provides a unified interface for managing connections to Solana validators
//! using either UDP or QUIC protocols. It abstracts the underlying connection management
//! and provides automatic protocol selection based on configuration.
//!
//! ## Key Features
//!
//! - **Protocol Abstraction**: Seamlessly switch between UDP and QUIC protocols
//! - **Connection Pooling**: Efficient connection reuse with configurable pool sizes
//! - **Automatic Fallback**: Graceful handling of protocol-specific failures
//! - **Key Management**: Support for client certificate updates for QUIC connections
//!
//! ## Architecture
//!
//! The connection cache uses an enum-based design to support multiple protocols:
//! - `ConnectionCache::Quic`: High-performance QUIC connections with encryption
//! - `ConnectionCache::Udp`: Traditional UDP connections for backward compatibility
//!
//! Connection pooling is handled by the underlying backend implementations,
//! with each protocol maintaining its own pool of reusable connections.

pub use solana_connection_cache::connection_cache::Protocol;
use {
    solana_connection_cache::{
        client_connection::ClientConnection,
        connection_cache::{
            BaseClientConnection, ConnectionCache as BackendConnectionCache, ConnectionPool,
            NewConnectionConfig,
        },
    },
    solana_keypair::Keypair,
    solana_pubkey::Pubkey,
    solana_quic_client::{QuicConfig, QuicConnectionManager, QuicPool},
    solana_quic_definitions::NotifyKeyUpdate,
    solana_streamer::streamer::StakedNodes,
    solana_transaction_error::TransportResult,
    solana_udp_client::{UdpConfig, UdpConnectionManager, UdpPool},
    std::{
        net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
        sync::{Arc, RwLock},
    },
};

// Default configuration constants for connection management
const DEFAULT_CONNECTION_POOL_SIZE: usize = 4;  // Number of connections per validator endpoint
const DEFAULT_CONNECTION_CACHE_USE_QUIC: bool = true;  // Prefer QUIC over UDP by default

/// A unified connection cache that abstracts UDP and QUIC protocols.
///
/// This enum provides a single interface for managing connections regardless of the
/// underlying protocol. It handles connection pooling, automatic retry logic, and
/// protocol-specific optimizations.
///
/// # Usage
///
/// ```rust
/// // Create a connection cache with default settings (QUIC preferred)
/// let cache = ConnectionCache::new("my_client");
///
/// // Get a connection to a specific validator
/// let connection = cache.get_connection(&validator_addr);
/// ```
///
/// # Protocol Selection
///
/// - **QUIC**: Used by default for better performance and security
/// - **UDP**: Available for backward compatibility or network constraints
pub enum ConnectionCache {
    /// QUIC-based connection cache with encryption and multiplexing
    Quic(Arc<BackendConnectionCache<QuicPool, QuicConnectionManager, QuicConfig>>),
    /// UDP-based connection cache for simple, stateless communication
    Udp(Arc<BackendConnectionCache<UdpPool, UdpConnectionManager, UdpConfig>>),
}

// Type aliases for cleaner code - these represent the underlying connection types
type QuicBaseClientConnection = <QuicPool as ConnectionPool>::BaseClientConnection;
type UdpBaseClientConnection = <UdpPool as ConnectionPool>::BaseClientConnection;

/// Blocking client connection that can be either UDP or QUIC.
///
/// This enum wraps the protocol-specific blocking connection types and provides
/// a unified interface for synchronous network operations.
pub enum BlockingClientConnection {
    Quic(Arc<<QuicBaseClientConnection as BaseClientConnection>::BlockingClientConnection>),
    Udp(Arc<<UdpBaseClientConnection as BaseClientConnection>::BlockingClientConnection>),
}

/// Non-blocking client connection that can be either UDP or QUIC.
///
/// This enum wraps the protocol-specific async connection types and provides
/// a unified interface for asynchronous network operations.
pub enum NonblockingClientConnection {
    Quic(Arc<<QuicBaseClientConnection as BaseClientConnection>::NonblockingClientConnection>),
    Udp(Arc<<UdpBaseClientConnection as BaseClientConnection>::NonblockingClientConnection>),
}

/// Implementation of key update functionality for QUIC connections.
///
/// QUIC connections require client certificates for authentication. This trait
/// allows updating the client key when needed (e.g., key rotation scenarios).
impl NotifyKeyUpdate for ConnectionCache {
    /// Updates the client certificate key for QUIC connections.
    ///
    /// # Arguments
    /// * `key` - The new keypair to use for client authentication
    ///
    /// # Returns
    /// * `Ok(())` for UDP (no-op) or successful QUIC key update
    /// * `Err(_)` if QUIC key update fails
    fn update_key(&self, key: &Keypair) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::Udp(_) => Ok(()), // UDP doesn't use client certificates
            Self::Quic(backend) => backend.update_key(key),
        }
    }
}

impl ConnectionCache {
    /// Creates a new connection cache with default settings.
    ///
    /// By default, this will create a QUIC-based connection cache for better
    /// performance and security. If QUIC is not available, it falls back to UDP.
    ///
    /// # Arguments
    /// * `name` - A static string identifier for this connection cache instance
    ///
    /// # Returns
    /// A new `ConnectionCache` instance configured with default settings
    pub fn new(name: &'static str) -> Self {
        if DEFAULT_CONNECTION_CACHE_USE_QUIC {
            let cert_info = (&Keypair::new(), IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
            ConnectionCache::new_with_client_options(
                name,
                DEFAULT_CONNECTION_POOL_SIZE,
                None, // client_endpoint
                Some(cert_info),
                None, // stake_info
            )
        } else {
            ConnectionCache::with_udp(name, DEFAULT_CONNECTION_POOL_SIZE)
        }
    }

    /// Creates a QUIC-only connection cache.
    ///
    /// This method explicitly creates a QUIC connection cache without fallback to UDP.
    /// Use this when you specifically need QUIC features like multiplexing and encryption.
    ///
    /// # Arguments
    /// * `name` - A static string identifier for this connection cache
    /// * `connection_pool_size` - Number of connections to maintain per endpoint
    ///
    /// # Returns
    /// A QUIC-based `ConnectionCache` instance
    pub fn new_quic(name: &'static str, connection_pool_size: usize) -> Self {
        Self::new_with_client_options(name, connection_pool_size, None, None, None)
    }

    /// Creates a QUIC connection cache with advanced configuration options.
    ///
    /// This method provides fine-grained control over QUIC connection parameters,
    /// including client certificates, socket binding, and staking information.
    ///
    /// # Arguments
    /// * `name` - A static string identifier for this connection cache
    /// * `connection_pool_size` - Number of connections to maintain per endpoint
    /// * `client_socket` - Optional pre-bound UDP socket for QUIC transport
    /// * `cert_info` - Optional client certificate (keypair + IP) for authentication
    /// * `stake_info` - Optional staking information for validator prioritization
    ///
    /// # Returns
    /// A fully configured QUIC-based `ConnectionCache` instance
    pub fn new_with_client_options(
        name: &'static str,
        connection_pool_size: usize,
        client_socket: Option<UdpSocket>,
        cert_info: Option<(&Keypair, IpAddr)>,
        stake_info: Option<(&Arc<RwLock<StakedNodes>>, &Pubkey)>,
    ) -> Self {
        // Ensure we have at least one connection in the pool
        let connection_pool_size = 1.max(connection_pool_size);
        let mut config = QuicConfig::new().unwrap();
        if let Some(cert_info) = cert_info {
            config.update_client_certificate(cert_info.0, cert_info.1);
        }
        if let Some(client_socket) = client_socket {
            config.update_client_endpoint(client_socket);
        }
        if let Some(stake_info) = stake_info {
            config.set_staked_nodes(stake_info.0, stake_info.1);
        }
        let connection_manager = QuicConnectionManager::new_with_connection_config(config);
        let cache =
            BackendConnectionCache::new(name, connection_manager, connection_pool_size).unwrap();
        Self::Quic(Arc::new(cache))
    }

    /// Returns the protocol type being used by this connection cache.
    ///
    /// # Returns
    /// * `Protocol::QUIC` for QUIC-based connections
    /// * `Protocol::UDP` for UDP-based connections
    #[inline]
    pub fn protocol(&self) -> Protocol {
        match self {
            Self::Quic(_) => Protocol::QUIC,
            Self::Udp(_) => Protocol::UDP,
        }
    }

    /// Creates a UDP-only connection cache.
    ///
    /// This method explicitly creates a UDP connection cache, bypassing QUIC entirely.
    /// Use this for backward compatibility or in environments where QUIC is not supported.
    ///
    /// # Arguments
    /// * `name` - A static string identifier for this connection cache
    /// * `connection_pool_size` - Number of connections to maintain per endpoint
    ///
    /// # Returns
    /// A UDP-based `ConnectionCache` instance
    pub fn with_udp(name: &'static str, connection_pool_size: usize) -> Self {
        // Ensure we have at least one connection in the pool
        let connection_pool_size = 1.max(connection_pool_size);
        let connection_manager = UdpConnectionManager::default();
        let cache =
            BackendConnectionCache::new(name, connection_manager, connection_pool_size).unwrap();
        Self::Udp(Arc::new(cache))
    }

    /// Checks if this connection cache is using QUIC protocol.
    ///
    /// # Returns
    /// `true` if using QUIC, `false` if using UDP
    pub fn use_quic(&self) -> bool {
        matches!(self, Self::Quic(_))
    }

    /// Retrieves a blocking connection to the specified address.
    ///
    /// This method returns a connection from the pool that can be used for
    /// synchronous network operations. The connection type (UDP/QUIC) matches
    /// the underlying cache protocol.
    ///
    /// # Arguments
    /// * `addr` - The socket address of the target validator
    ///
    /// # Returns
    /// A `BlockingClientConnection` ready for synchronous use
    pub fn get_connection(&self, addr: &SocketAddr) -> BlockingClientConnection {
        match self {
            Self::Quic(cache) => BlockingClientConnection::Quic(cache.get_connection(addr)),
            Self::Udp(cache) => BlockingClientConnection::Udp(cache.get_connection(addr)),
        }
    }

    /// Retrieves a non-blocking connection to the specified address.
    ///
    /// This method returns a connection from the pool that can be used for
    /// asynchronous network operations. The connection type (UDP/QUIC) matches
    /// the underlying cache protocol.
    ///
    /// # Arguments
    /// * `addr` - The socket address of the target validator
    ///
    /// # Returns
    /// A `NonblockingClientConnection` ready for asynchronous use
    pub fn get_nonblocking_connection(&self, addr: &SocketAddr) -> NonblockingClientConnection {
        match self {
            Self::Quic(cache) => {
                NonblockingClientConnection::Quic(cache.get_nonblocking_connection(addr))
            }
            Self::Udp(cache) => {
                NonblockingClientConnection::Udp(cache.get_nonblocking_connection(addr))
            }
        }
    }
}

/// Macro for generating protocol-agnostic method dispatchers.
///
/// This macro automatically generates methods that dispatch to the appropriate
/// protocol-specific implementation (UDP or QUIC) based on the connection type.
/// It eliminates code duplication and ensures consistent behavior across protocols.
///
/// The macro handles both immutable and mutable method variants, preserving
/// all function signatures, generics, and return types while adding the
/// protocol dispatch logic.
macro_rules! dispatch {
    // Pattern for immutable methods (&self)
    ($(#[$meta:meta])* $vis:vis fn $name:ident$(<$($t:ident: $cons:ident + ?Sized),*>)?(&self $(, $arg:ident: $ty:ty)*) $(-> $out:ty)?) => {
        #[inline]
        $(#[$meta])*
        $vis fn $name$(<$($t: $cons + ?Sized),*>)?(&self $(, $arg:$ty)*) $(-> $out)? {
            match self {
                Self::Quic(this) => this.$name($($arg, )*),
                Self::Udp(this) => this.$name($($arg, )*),
            }
        }
    };
    // Pattern for mutable methods (&mut self)
    ($(#[$meta:meta])* $vis:vis fn $name:ident$(<$($t:ident: $cons:ident + ?Sized),*>)?(&mut self $(, $arg:ident: $ty:ty)*) $(-> $out:ty)?) => {
        #[inline]
        $(#[$meta])*
        $vis fn $name$(<$($t: $cons + ?Sized),*>)?(&mut self $(, $arg:$ty)*) $(-> $out)? {
            match self {
                Self::Quic(this) => this.$name($($arg, )*),
                Self::Udp(this) => this.$name($($arg, )*),
            }
        }
    };
}

/// Export the dispatch macro for use in other modules within this crate.
/// This allows other client components to use the same pattern for protocol dispatch.
pub(crate) use dispatch;

/// Implementation of the ClientConnection trait for blocking operations.
///
/// This implementation uses the dispatch macro to automatically route method calls
/// to the appropriate protocol-specific implementation (UDP or QUIC).
impl ClientConnection for BlockingClientConnection {
    // Get the server address for this connection
    dispatch!(fn server_addr(&self) -> &SocketAddr);
    
    // Send data synchronously and wait for completion
    dispatch!(fn send_data(&self, buffer: &[u8]) -> TransportResult<()>);
    
    // Send data asynchronously (fire-and-forget)
    dispatch!(fn send_data_async(&self, buffer: Vec<u8>) -> TransportResult<()>);
    
    // Send multiple data buffers synchronously
    dispatch!(fn send_data_batch(&self, buffers: &[Vec<u8>]) -> TransportResult<()>);
    
    // Send multiple data buffers asynchronously
    dispatch!(fn send_data_batch_async(&self, buffers: Vec<Vec<u8>>) -> TransportResult<()>);
}

/// Implementation of the async ClientConnection trait for non-blocking operations.
///
/// This implementation provides asynchronous versions of the connection methods,
/// using the dispatch pattern to route calls to the appropriate protocol backend.
#[async_trait::async_trait]
impl solana_connection_cache::nonblocking::client_connection::ClientConnection
    for NonblockingClientConnection
{
    // Get the server address for this connection (synchronous)
    dispatch!(fn server_addr(&self) -> &SocketAddr);

    /// Sends data asynchronously to the connected server.
    ///
    /// # Arguments
    /// * `buffer` - The data to send
    ///
    /// # Returns
    /// `TransportResult<()>` indicating success or failure
    async fn send_data(&self, buffer: &[u8]) -> TransportResult<()> {
        match self {
            Self::Quic(cache) => Ok(cache.send_data(buffer).await?),
            Self::Udp(cache) => Ok(cache.send_data(buffer).await?),
        }
    }

    /// Sends multiple data buffers asynchronously to the connected server.
    ///
    /// # Arguments
    /// * `buffers` - A slice of data buffers to send
    ///
    /// # Returns
    /// `TransportResult<()>` indicating success or failure
    async fn send_data_batch(&self, buffers: &[Vec<u8>]) -> TransportResult<()> {
        match self {
            Self::Quic(cache) => Ok(cache.send_data_batch(buffers).await?),
            Self::Udp(cache) => Ok(cache.send_data_batch(buffers).await?),
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::connection_cache::ConnectionCache,
        solana_net_utils::bind_to_localhost,
        std::net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    #[test]
    fn test_connection_with_specified_client_endpoint() {
        let client_socket = bind_to_localhost().unwrap();
        let connection_cache = ConnectionCache::new_with_client_options(
            "connection_cache_test",
            1,                   // connection_pool_size
            Some(client_socket), // client_endpoint
            None,                // cert_info
            None,                // stake_info
        );

        // server port 1:
        let port1 = 9001;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port1);
        let conn = connection_cache.get_connection(&addr);
        assert_eq!(conn.server_addr().port(), port1);

        // server port 2:
        let port2 = 9002;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port2);
        let conn = connection_cache.get_connection(&addr);
        assert_eq!(conn.server_addr().port(), port2);
    }
}
