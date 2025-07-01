/// Jito-Solana Validator Library
/// 
/// This module provides core utilities and shared functionality for the Jito-Solana validator.
/// It serves as the foundation library that other validator components depend on for common
/// operations such as UI formatting, progress indication, and file locking.
/// 
/// Key functionality:
/// - Progress bar and status display utilities for user feedback
/// - Ledger file locking to prevent multiple validator instances
/// - Console formatting utilities for consistent output styling
/// - Test validator re-export for development and testing
/// 
/// The library ensures consistent user experience across all validator operations
/// and provides safe concurrent access to shared resources like the ledger directory.

#![allow(clippy::arithmetic_side_effects)]
pub use solana_test_validator as test_validator;
use {
    console::style,
    fd_lock::{RwLock, RwLockWriteGuard},
    indicatif::{ProgressDrawTarget, ProgressStyle},
    std::{
        borrow::Cow,
        fmt::Display,
        fs::{File, OpenOptions},
        path::Path,
        process::exit,
        time::Duration,
    },
};

pub mod admin_rpc_service;
pub mod bootstrap;
pub mod cli;
pub mod commands;
pub mod dashboard;

/// Format a name-value pair with consistent styling
/// 
/// Creates a formatted string with the name portion bold-styled and separated
/// from the value. This provides consistent visual formatting across all
/// validator output for configuration display and status reporting.
/// 
/// # Arguments
/// * `name` - The label/key to display (will be bold)
/// * `value` - The associated value to display
/// 
/// # Returns
/// A formatted string ready for display
pub fn format_name_value(name: &str, value: &str) -> String {
    format!("{} {}", style(name).bold(), value)
}

/// Print a formatted name-value pair to stdout
/// 
/// Convenience function that combines formatting and printing for immediate
/// display of configuration or status information. Used throughout the validator
/// for consistent output formatting.
/// 
/// # Arguments
/// * `name` - The label/key to display
/// * `value` - The associated value to display
pub fn println_name_value(name: &str, value: &str) {
    println!("{}", format_name_value(name, value));
}

/// Creates a new spinner progress bar for indeterminate operations
/// 
/// This function creates a customized progress bar suitable for operations where
/// the completion time is unknown (like network operations, file downloads, or
/// validator startup). The spinner provides visual feedback that the process
/// is active while waiting for completion.
/// 
/// Features:
/// - Green animated spinner for visual appeal
/// - Wide message area for status updates
/// - Terminal detection for appropriate output mode
/// - 100ms refresh rate for smooth animation
/// 
/// # Returns
/// A configured ProgressBar wrapper that handles both terminal and non-terminal output
pub fn new_spinner_progress_bar() -> ProgressBar {
    let progress_bar = indicatif::ProgressBar::new(42);
    progress_bar.set_draw_target(ProgressDrawTarget::stdout());
    progress_bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {wide_msg}")
            .expect("ProgresStyle::template direct input to be correct"),
    );
    progress_bar.enable_steady_tick(Duration::from_millis(100));

    ProgressBar {
        progress_bar,
        is_term: console::Term::stdout().is_term(),
    }
}

/// Wrapper for progress bar that adapts to terminal vs non-terminal output
/// 
/// This struct provides a unified interface for progress indication that automatically
/// adapts its behavior based on whether output is going to a terminal or being redirected.
/// In terminal mode, it shows animated progress bars; in non-terminal mode, it falls back
/// to simple text output for logging compatibility.
pub struct ProgressBar {
    /// The underlying indicatif progress bar for terminal display
    progress_bar: indicatif::ProgressBar,
    /// Whether output is going to a terminal (true) or being redirected (false)
    is_term: bool,
}

impl ProgressBar {
    /// Set the current status message for the progress bar
    /// 
    /// In terminal mode, this updates the progress bar's message area with the new status.
    /// In non-terminal mode (redirected output), it prints the message as a regular line
    /// to ensure status updates are captured in logs.
    /// 
    /// # Arguments
    /// * `msg` - The status message to display
    pub fn set_message<T: Into<Cow<'static, str>> + Display>(&self, msg: T) {
        if self.is_term {
            self.progress_bar.set_message(msg);
        } else {
            println!("{msg}");
        }
    }

    /// Print a line of output above the progress bar
    /// 
    /// This method ensures that text output doesn't interfere with the progress bar
    /// display by properly positioning the message above the progress area.
    /// 
    /// # Arguments
    /// * `msg` - The message to print
    pub fn println<I: AsRef<str>>(&self, msg: I) {
        self.progress_bar.println(msg);
    }

    /// Finalize the progress bar with a completion message
    /// 
    /// This stops the progress bar animation and displays a final status message.
    /// In terminal mode, it abandons the progress bar cleanly; in non-terminal mode,
    /// it simply prints the final message.
    /// 
    /// # Arguments
    /// * `msg` - The final completion or error message
    pub fn abandon_with_message<T: Into<Cow<'static, str>> + Display>(&self, msg: T) {
        if self.is_term {
            self.progress_bar.abandon_with_message(msg);
        } else {
            println!("{msg}");
        }
    }
}

/// Create a file-based read-write lock for the ledger directory
/// 
/// This function creates a lockfile within the ledger directory to prevent multiple
/// validator instances from accessing the same ledger simultaneously. The lock ensures
/// data integrity and prevents corruption that could occur from concurrent access.
/// 
/// The lockfile (`ledger.lock`) is created within the ledger directory and uses
/// the operating system's file locking mechanisms to provide exclusive access control.
/// 
/// # Arguments
/// * `ledger_path` - Path to the ledger directory
/// 
/// # Returns
/// An RwLock wrapping the lockfile handle
/// 
/// # Panics
/// Panics if unable to create or open the lockfile
pub fn ledger_lockfile(ledger_path: &Path) -> RwLock<File> {
    let lockfile = ledger_path.join("ledger.lock");
    fd_lock::RwLock::new(
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lockfile)
            .unwrap(),
    )
}

/// Acquire an exclusive write lock on the ledger directory
/// 
/// This function attempts to acquire an exclusive lock on the ledger directory
/// to prevent multiple validator instances from running with the same ledger.
/// If the lock cannot be acquired (because another validator is running),
/// the function prints an error message and exits the process.
/// 
/// This is a critical safety mechanism that prevents data corruption and
/// ensures only one validator instance can access a ledger at a time.
/// 
/// # Arguments
/// * `ledger_path` - Path to the ledger directory (for error messages)
/// * `ledger_lockfile` - Mutable reference to the lockfile RwLock
/// 
/// # Returns
/// A write guard that holds the exclusive lock for the lifetime of the guard
/// 
/// # Panics/Exits
/// Exits the process (exit code 1) if unable to acquire the lock
pub fn lock_ledger<'lock>(
    ledger_path: &Path,
    ledger_lockfile: &'lock mut RwLock<File>,
) -> RwLockWriteGuard<'lock, File> {
    ledger_lockfile.try_write().unwrap_or_else(|_| {
        println!(
            "Error: Unable to lock {} directory. Check if another validator is running",
            ledger_path.display()
        );
        exit(1);
    })
}
