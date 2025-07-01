#![allow(clippy::arithmetic_side_effects)]
#[cfg(not(any(target_env = "msvc", target_os = "freebsd")))]
use jemallocator::Jemalloc;
use {
    agave_validator::{
        cli::{app, warn_for_deprecated_arguments, DefaultArgs},
        commands,
    },
    log::error,
    solana_streamer::socket::SocketAddrSpace,
    std::{path::PathBuf, process::exit},
};

#[cfg(not(any(target_env = "msvc", target_os = "freebsd")))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

/// Main entry point for the Jito-Solana validator
/// 
/// This function serves as the primary entry point for the validator binary.
/// It handles command-line argument parsing and routes execution to specific
/// validator operations based on the provided subcommand.
/// 
/// Workflow:
/// 1. Initialize default CLI arguments and get version info
/// 2. Parse command-line arguments using clap
/// 3. Validate deprecated arguments and warn users
/// 4. Determine network address space restrictions
/// 5. Extract ledger path and route to appropriate subcommand handler
/// 
/// Supported subcommands:
/// - `init`: Initialize ledger directory and prepare for startup
/// - `run` (default): Start the validator in normal operation mode
/// - `authorized-voter`: Manage authorized voter keypairs
/// - `contact-info`: Display validator network contact information
/// - `exit`: Gracefully shutdown the validator
/// - `monitor`: Display real-time validator status dashboard
/// - `plugin`: Manage Geyser plugins
/// - Various MEV-specific commands (block-engine, relayer, shred management)
pub fn main() {
    let default_args = DefaultArgs::new();
    let solana_version = solana_version::version!();
    let cli_app = app(solana_version, &default_args);
    let matches = cli_app.get_matches();
    warn_for_deprecated_arguments(&matches);

    // Configure network address space restrictions based on CLI flags
    // This determines whether private/local addresses are allowed for network communication
    let socket_addr_space = SocketAddrSpace::new(matches.is_present("allow_private_addr"));
    
    // Extract the ledger path where blockchain data will be stored
    // This is a required argument for all validator operations
    let ledger_path = PathBuf::from(matches.value_of("ledger_path").unwrap());

    // Route execution based on the specified subcommand
    // Each subcommand represents a different validator operation mode
    match matches.subcommand() {
        // Initialize validator ledger directory and configuration
        // This prepares the validator for first-time startup but doesn't start services
        ("init", _) => commands::run::execute(
            &matches,
            solana_version,
            socket_addr_space,
            &ledger_path,
            commands::run::execute::Operation::Initialize,
        )
        .inspect_err(|err| error!("Failed to initialize validator: {err}"))
        .map_err(commands::Error::Dynamic),
        
        // Default operation: run the validator in full operational mode
        // This starts all validator services and begins transaction processing
        ("", _) | ("run", _) => commands::run::execute(
            &matches,
            solana_version,
            socket_addr_space,
            &ledger_path,
            commands::run::execute::Operation::Run,
        )
        .inspect_err(|err| error!("Failed to start validator: {err}"))
        .map_err(commands::Error::Dynamic),
        ("authorized-voter", Some(authorized_voter_subcommand_matches)) => {
            commands::authorized_voter::execute(authorized_voter_subcommand_matches, &ledger_path)
        }
        ("plugin", Some(plugin_subcommand_matches)) => {
            commands::plugin::execute(plugin_subcommand_matches, &ledger_path)
        }
        ("contact-info", Some(subcommand_matches)) => {
            commands::contact_info::execute(subcommand_matches, &ledger_path)
        }
        ("exit", Some(subcommand_matches)) => {
            commands::exit::execute(subcommand_matches, &ledger_path)
        }
        ("monitor", _) => commands::monitor::execute(&matches, &ledger_path),
        ("staked-nodes-overrides", Some(subcommand_matches)) => {
            commands::staked_nodes_overrides::execute(subcommand_matches, &ledger_path)
        }
        ("set-identity", Some(subcommand_matches)) => {
            commands::set_identity::execute(subcommand_matches, &ledger_path)
        }
        ("set-log-filter", Some(subcommand_matches)) => {
            commands::set_log_filter::execute(subcommand_matches, &ledger_path)
        }
        ("wait-for-restart-window", Some(subcommand_matches)) => {
            commands::wait_for_restart_window::execute(subcommand_matches, &ledger_path)
        }
        ("repair-shred-from-peer", Some(subcommand_matches)) => {
            commands::repair_shred_from_peer::execute(subcommand_matches, &ledger_path)
        }
        ("repair-whitelist", Some(repair_whitelist_subcommand_matches)) => {
            commands::repair_whitelist::execute(repair_whitelist_subcommand_matches, &ledger_path)
        }
        ("set-public-address", Some(subcommand_matches)) => {
            commands::set_public_address::execute(subcommand_matches, &ledger_path)
        }
        ("set-block-engine-config", Some(subcommand_matches)) => {
            commands::block_engine::execute(subcommand_matches, &ledger_path)
        }
        ("set-relayer-config", Some(subcommand_matches)) => {
            commands::relayer::execute(subcommand_matches, &ledger_path)
        }
        ("set-shred-receiver-address", Some(subcommand_matches)) => {
            commands::shred::set_shred_receiver_execute(subcommand_matches, &ledger_path)
        }
        ("set-shred-retransmit-receiver-address", Some(subcommand_matches)) => {
            commands::shred::set_shred_retransmit_receiver_execute(subcommand_matches, &ledger_path)
        }
        ("runtime-plugin", Some(plugin_subcommand_matches)) => {
            commands::runtime_plugin::execute(plugin_subcommand_matches, &ledger_path)
        }
        _ => unreachable!(),
    }
    .unwrap_or_else(|err| {
        println!("Validator command failed: {err}");
        exit(1);
    })
}
