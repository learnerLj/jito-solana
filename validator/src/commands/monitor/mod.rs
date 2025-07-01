/// Real-Time Validator Monitoring Command
/// 
/// This module implements the `monitor` subcommand that provides a real-time
/// dashboard for observing validator status, performance metrics, and network
/// connectivity. The monitor is essential for validator operators to track
/// their validator's health and performance.
/// 
/// Features provided by the monitoring dashboard:
/// - Live slot progression (processed, confirmed, finalized)
/// - Transaction throughput and success rates
/// - Network connectivity and peer information
/// - Snapshot download/creation progress
/// - Validator identity and version details
/// - System health indicators and error states
/// 
/// The monitor connects to the running validator through the admin RPC interface
/// and refreshes data every 2 seconds by default. It gracefully handles
/// connection failures and validator restarts, making it suitable for
/// continuous monitoring in production environments.

use {
    crate::{commands::Result, dashboard::Dashboard},
    clap::{App, ArgMatches, SubCommand},
    std::{path::Path, time::Duration},
};

pub fn command<'a>() -> App<'a, 'a> {
    SubCommand::with_name("monitor").about("Monitor the validator")
}

/// Execute the monitor command with the given arguments
/// 
/// This function serves as the entry point for the monitor subcommand,
/// setting up and launching the real-time validator dashboard.
/// 
/// # Arguments
/// * `_matches` - Command-line arguments (currently unused)
/// * `ledger_path` - Path to the validator's ledger directory
/// 
/// # Returns
/// Result indicating success or failure of the monitoring operation
pub fn execute(_matches: &ArgMatches, ledger_path: &Path) -> Result<()> {
    monitor_validator(ledger_path)
}

/// Launch the validator monitoring dashboard
/// 
/// Creates and runs a real-time dashboard that connects to the validator
/// through the admin RPC interface. The dashboard updates every 2 seconds
/// and provides comprehensive status information about the validator's
/// operation, performance, and network connectivity.
/// 
/// The function blocks until the user interrupts the monitoring session
/// (typically with Ctrl+C), making it suitable for interactive monitoring.
/// 
/// # Arguments
/// * `ledger_path` - Path to the validator's ledger directory for admin RPC connection
/// 
/// # Returns
/// Result indicating successful completion of monitoring (typically after user interruption)
pub fn monitor_validator(ledger_path: &Path) -> Result<()> {
    let dashboard = Dashboard::new(ledger_path, None, None);
    dashboard.run(Duration::from_secs(2));

    Ok(())
}
