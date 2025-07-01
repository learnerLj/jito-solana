/// Validator Administrative Commands Module
/// 
/// This module contains all subcommands available for validator operation and administration.
/// Each subcommand provides specific functionality for managing different aspects of a
/// running or offline validator instance.
/// 
/// Command categories:
/// 
/// **Core Operations:**
/// - `run`: Main validator operation (transaction processing, consensus participation)
/// - `monitor`: Real-time status monitoring and performance dashboards
/// - `exit`: Graceful validator shutdown
/// 
/// **Identity & Security:**
/// - `authorized_voter`: Manage voting keypairs for consensus participation
/// - `set_identity`: Update validator identity keypair for network identification
/// 
/// **Network Configuration:**
/// - `contact_info`: Display validator network contact information
/// - `set_public_address`: Configure public network addresses for external access
/// - `repair_shred_from_peer`: Request specific ledger data repair from network peers
/// - `repair_whitelist`: Manage trusted peer list for ledger repair operations
/// 
/// **MEV Integration (Jito-specific):**
/// - `block_engine`: Configure connection to MEV block construction engine
/// - `relayer`: Set up transaction relayer for MEV transaction forwarding
/// - `shred`: Configure specialized shred routing for MEV optimization
/// 
/// **Plugin Management:**
/// - `plugin`: Manage Geyser plugins for custom transaction/account processing
/// - `runtime_plugin`: Configure runtime plugins for transaction execution hooks
/// 
/// **Advanced Administration:**
/// - `set_log_filter`: Adjust logging verbosity at runtime
/// - `staked_nodes_overrides`: Override stake weights for specific validators
/// - `wait_for_restart_window`: Wait for optimal cluster restart timing

pub mod authorized_voter;
pub mod block_engine;
pub mod contact_info;
pub mod exit;
pub mod monitor;
pub mod plugin;
pub mod relayer;
pub mod repair_shred_from_peer;
pub mod repair_whitelist;
pub mod run;
pub mod runtime_plugin;
pub mod set_identity;
pub mod set_log_filter;
pub mod set_public_address;
pub mod shred;
pub mod staked_nodes_overrides;
pub mod wait_for_restart_window;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("admin rpc error: {0}")]
    AdminRpc(#[from] jsonrpc_core_client::RpcError),

    #[error(transparent)]
    Clap(#[from] clap::Error),

    #[error(transparent)]
    Dynamic(#[from] Box<dyn std::error::Error>),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

pub trait FromClapArgMatches {
    fn from_clap_arg_match(matches: &clap::ArgMatches) -> Result<Self>
    where
        Self: Sized;
}

#[cfg(test)]
pub mod tests {
    use std::fmt::Debug;

    pub fn verify_args_struct_by_command<T>(app: clap::App, vec: Vec<&str>, expected_arg: T)
    where
        T: crate::commands::FromClapArgMatches + PartialEq + Debug,
    {
        let matches = app.get_matches_from(vec);
        let result = T::from_clap_arg_match(&matches);
        assert_eq!(result.unwrap(), expected_arg);
    }

    pub fn verify_args_struct_by_command_is_error<T>(app: clap::App, vec: Vec<&str>)
    where
        T: crate::commands::FromClapArgMatches + PartialEq + Debug,
    {
        let matches = app.get_matches_from_safe(vec);
        assert!(matches.is_err());
    }
}
