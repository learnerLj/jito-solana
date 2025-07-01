#![cfg_attr(feature = "frozen-abi", feature(min_specialization))]
#![allow(clippy::arithmetic_side_effects)]

//! # Solana Virtual Machine (SVM)
//! 
//! The SVM is the transaction execution engine for the Solana blockchain. It provides
//! a sandboxed environment for executing smart contracts (programs) and processing
//! transactions while maintaining security, determinism, and performance.
//! 
//! ## Architecture Overview
//! 
//! The SVM consists of several key components:
//! 
//! - **Transaction Processor**: Main orchestrator for transaction execution
//! - **Account Loader**: Manages loading and validation of account data
//! - **Program Loader**: Handles loading and caching of executable programs
//! - **Message Processor**: Executes individual instructions within transactions
//! - **Balance Tracking**: Monitors account balance changes during execution
//! 
//! ## Key Features
//! 
//! - **Parallel Execution**: Supports concurrent transaction processing
//! - **Program Caching**: Efficient caching of compiled programs
//! - **Account State Management**: Tracks account changes with rollback capability
//! - **Fee Collection**: Handles transaction fees and rent collection
//! - **Nonce Support**: Enables durable transaction signatures

/// Account loading and validation functionality
/// Responsible for loading account data from storage and validating access permissions
pub mod account_loader;

/// Account state override functionality for simulation
/// Allows temporary account state modifications for transaction simulation
pub mod account_overrides;

/// Core instruction processing logic
/// Handles the execution of individual instructions within transactions
pub mod message_processor;

/// Nonce account handling for durable transactions
/// Manages nonce accounts that enable offline transaction signing
pub mod nonce_info;

/// Program loading and caching system
/// Handles loading, compilation, and caching of executable programs
pub mod program_loader;

/// Account state rollback functionality
/// Manages reverting account changes when transactions fail
pub mod rollback_accounts;

/// Transaction account state tracking
/// Monitors account state changes during transaction execution
pub mod transaction_account_state_info;

/// Balance collection and tracking
/// Records account balance changes for reporting and validation
pub mod transaction_balances;

/// Transaction commit result types
/// Defines the results of committing transaction changes to state
pub mod transaction_commit_result;

/// Transaction error metrics and reporting
/// Collects and reports various transaction error statistics
pub mod transaction_error_metrics;

/// Transaction execution result types
/// Defines the outcomes of transaction execution attempts
pub mod transaction_execution_result;

/// Callback interface for transaction processing
/// Provides hooks for external systems to interact with the SVM
pub mod transaction_processing_callback;

/// Transaction processing result types
/// Defines the overall results of transaction processing
pub mod transaction_processing_result;

/// Main transaction processing engine
/// The central coordinator for all transaction execution activities
pub mod transaction_processor;

#[cfg_attr(feature = "frozen-abi", macro_use)]
#[cfg(feature = "frozen-abi")]
extern crate solana_frozen_abi_macro;
