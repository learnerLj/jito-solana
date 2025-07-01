# Solana Virtual Machine (SVM)

The Solana Virtual Machine (SVM) is the transaction execution engine for the Solana blockchain. It provides a sandboxed environment for executing smart contracts (programs) and processing transactions while maintaining security, determinism, and performance.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Core Components](#core-components)
- [Transaction Processing Workflow](#transaction-processing-workflow)
- [Program Loading and Caching](#program-loading-and-caching)
- [Account Management](#account-management)
- [Error Handling and Rollback](#error-handling-and-rollback)
- [Performance and Metrics](#performance-and-metrics)
- [Usage Examples](#usage-examples)
- [API Reference](#api-reference)

## Overview

The SVM is designed to be a standalone, reusable transaction execution engine that can operate independently of the full Solana validator. This modular design enables various use cases:

### Primary Use Cases

1. **Transaction execution in Solana Validator** - Core component of the Agave validator
2. **SVM Rollups** - Lightweight execution for layer-2 solutions
3. **SVM Fraud Proofs** - Verification for diet clients (SIMD-65)
4. **Validator Sidecar for JSON-RPC** - Isolated execution for RPC services
5. **Custom Blockchain Implementations** - Reusable execution engine for new chains

### Key Features

- **Parallel Execution**: Supports concurrent transaction processing
- **Program Caching**: Efficient caching of compiled BPF programs
- **Account State Management**: Tracks account changes with atomic rollback capability
- **Fee Collection**: Handles transaction fees and rent collection
- **Nonce Support**: Enables durable transaction signatures for offline signing
- **Multi-Loader Support**: Compatible with all Solana program loader versions

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Transaction Batch Processor                  │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐   │
│  │  Account Loader │ │ Program Loader  │ │ Message Processor│   │
│  │                 │ │                 │ │                 │   │
│  │ • Load accounts │ │ • Load programs │ │ • Execute instr │   │
│  │ • Validate perms│ │ • Cache compiled│ │ • Manage compute│   │
│  │ • Fee validation│ │ • Version check │ │ • Error handling│   │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Supporting Systems                          │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐   │
│  │ Balance Tracker │ │ Rollback Manager│ │ Metrics Collector│   │
│  │                 │ │                 │ │                 │   │
│  │ • Pre/post bal  │ │ • Nonce accounts│ │ • Timing data   │   │
│  │ • Token tracking│ │ • Fee rollback  │ │ • Error metrics │   │
│  │ • Native balances│ │ • State restore │ │ • Compute units │   │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### Transaction Processor (`transaction_processor.rs`)

The central orchestrator that coordinates all transaction execution activities:

- **TransactionBatchProcessor**: Main entry point for transaction processing
- **Environment Management**: Handles runtime configuration and feature flags
- **Batch Processing**: Processes multiple transactions efficiently
- **Result Aggregation**: Collects and reports execution results

### Account Loader (`account_loader.rs`)

Manages loading and validation of account data required for transaction execution:

- **Account Loading**: Retrieves account data from storage via callback interface
- **Permission Validation**: Verifies signer/writable permissions
- **Fee Validation**: Ensures sufficient balance for transaction fees
- **Rent Collection**: Handles rent debits and exemption calculations

### Program Loader (`program_loader.rs`)

Handles loading, compilation, and caching of executable programs:

- **Multi-Loader Support**: Compatible with Loader V1, V2, V3 (upgradeable), and V4
- **Program Compilation**: Compiles ELF bytecode to executable BPF programs
- **Caching System**: Maintains compiled program cache for performance
- **Version Management**: Tracks program deployment slots for cache invalidation

### Message Processor (`message_processor.rs`)

Executes individual instructions within transactions:

- **Instruction Execution**: Processes each instruction sequentially
- **Compute Management**: Tracks and limits compute unit consumption
- **Precompile Handling**: Routes native programs (ed25519, secp256k1) appropriately
- **Error Context**: Provides detailed error information with instruction indices

### Account Management

#### Rollback Accounts (`rollback_accounts.rs`)
Manages reverting account changes when transactions fail:

- **Fee Payer Rollback**: Restores fee payer account state on failure
- **Nonce Account Rollback**: Preserves nonce state for durable transactions
- **State Capture**: Captures pre-execution state for recovery

#### Balance Tracking (`transaction_balances.rs`)
Records account balance changes for reporting and validation:

- **Native Balance Tracking**: Monitors SOL balance changes
- **Token Balance Tracking**: Tracks SPL token account balances
- **Pre/Post Comparison**: Captures before and after transaction state

### Error Handling and Metrics

#### Error Metrics (`transaction_error_metrics.rs`)
Collects comprehensive error statistics:

- **Error Classification**: Categorizes errors by type and frequency
- **Performance Impact**: Tracks error-related performance metrics
- **Debugging Support**: Provides detailed error context

#### Execution Results (`transaction_execution_result.rs`, `transaction_processing_result.rs`)
Defines comprehensive result types:

- **Execution Details**: Captures compute units, logs, inner instructions
- **Processing Outcomes**: Distinguishes between executed and fees-only transactions
- **Status Tracking**: Provides success/failure determination

## Transaction Processing Workflow

### Phase 1: Preparation and Loading

1. **Transaction Validation**
   - Signature verification (pre-SVM)
   - Basic transaction structure validation
   - Account index validation

2. **Program Cache Preparation**
   ```rust
   // Filter executable programs and build program accounts map
   let program_accounts = filter_executable_program_accounts(&transactions);
   
   // Replenish program cache with required programs
   let program_cache = replenish_program_cache(program_accounts, &mut load_metrics);
   ```

3. **Account Loading**
   ```rust
   // Load all required accounts for the transaction batch
   for (transaction, check_result) in transactions.iter().zip(check_results.iter()) {
       let loaded_transaction = load_transaction_accounts(
           transaction,
           check_result,
           &account_loader,
           &rent_collector,
       )?;
   }
   ```

### Phase 2: Transaction Execution

4. **Pre-Execution State Capture**
   ```rust
   // Capture account balances and rent state before execution
   let pre_balances = collect_balances(&account_loader, &transaction);
   let pre_rent_state = capture_rent_state(&loaded_accounts);
   ```

5. **Message Processing**
   ```rust
   // Execute each instruction in the transaction
   for (instruction_index, (program_id, instruction)) in message.instructions_iter().enumerate() {
       if invoke_context.is_precompile(program_id) {
           // Handle native precompiled programs
           process_precompile(program_id, instruction_data, &instruction_accounts);
       } else {
           // Handle BPF programs
           process_instruction(instruction_data, &instruction_accounts, &mut compute_units);
       }
   }
   ```

6. **Post-Execution Validation**
   ```rust
   // Verify rent state transitions are valid
   verify_rent_state_changes(&pre_rent_state, &post_rent_state)?;
   
   // Ensure account balance invariants are maintained
   verify_balance_invariants(&pre_balances, &post_balances)?;
   ```

### Phase 3: Result Processing and Cleanup

7. **Result Aggregation**
   ```rust
   // Collect execution results
   let execution_result = TransactionExecutionResult {
       status: execution_status,
       log_messages: extract_log_messages(&log_collector),
       inner_instructions: extract_inner_instructions(&transaction_context),
       return_data: extract_return_data(&transaction_context),
       executed_units: total_compute_units_consumed,
   };
   ```

8. **Error Handling and Rollback**
   ```rust
   // If transaction failed, rollback account changes
   if execution_result.status.is_err() {
       rollback_accounts.restore_account_state(&mut account_loader);
   }
   ```

## Program Loading and Caching

### Loader Types

The SVM supports multiple program loader versions, each with different capabilities:

#### Loader V1 (Deprecated) - `bpf_loader_deprecated`
- Legacy loader with basic functionality
- No upgrade capability
- Security limitations

#### Loader V2 - `bpf_loader`  
- Standard BPF loader with improved security
- Immutable programs
- Better validation

#### Loader V3 (Upgradeable) - `bpf_loader_upgradeable`
- Supports program upgrades
- Separate program and programdata accounts
- Authority-based upgrade control

#### Loader V4 - `loader_v4`
- Latest loader with enhanced features
- Improved performance and security
- Advanced deployment options

### Caching Strategy

```rust
// Program cache entry lifecycle
pub enum ProgramCacheEntryType {
    Loaded(LoadedProgram),           // Successfully compiled and cached
    FailedVerification(Environment), // Failed to verify/compile  
    Closed,                         // Program account is closed
    DelayVisibility,               // Program not yet visible
}

// Cache management
impl ProgramCache {
    fn extract_or_load(&mut self, pubkey: &Pubkey, slot: Slot) -> Arc<ProgramCacheEntry> {
        // 1. Check if program is already in cache
        if let Some(cached_entry) = self.find(pubkey, slot) {
            return cached_entry;
        }
        
        // 2. Load program from account data
        let program_account = load_program_accounts(pubkey)?;
        
        // 3. Compile and cache the program
        let compiled_program = compile_program(program_account)?;
        self.assign_program(pubkey, compiled_program)
    }
}
```

## Account Management

### Account Loading Process

```rust
// Account loading workflow
fn load_transaction_accounts(
    transaction: &SanitizedTransaction,
    check_result: &TransactionCheckResult,
    account_loader: &AccountLoader,
    rent_collector: &RentCollector,
) -> Result<LoadedTransaction, TransactionError> {
    
    // 1. Load fee payer account
    let fee_payer = load_and_validate_fee_payer(
        transaction.message().fee_payer(),
        transaction.message().recent_blockhash(),
        check_result.lamports_per_signature,
    )?;
    
    // 2. Load all transaction accounts
    let mut loaded_accounts = Vec::new();
    for account_key in transaction.message().account_keys() {
        let account = account_loader.load_account(account_key)
            .ok_or(TransactionError::AccountNotFound)?;
        loaded_accounts.push((account_key, account));
    }
    
    // 3. Validate account permissions
    validate_account_permissions(&transaction.message(), &loaded_accounts)?;
    
    // 4. Calculate and collect rent
    let rent_debits = collect_rent(&loaded_accounts, rent_collector)?;
    
    Ok(LoadedTransaction {
        accounts: loaded_accounts,
        rent_debits,
        fee_details: calculate_fee_details(&fee_payer, check_result),
    })
}
```

### Balance Tracking

The SVM tracks both native SOL balances and SPL token balances:

```rust
// Balance collection for native accounts
fn collect_native_balances(accounts: &[AccountSharedData]) -> Vec<u64> {
    accounts.iter().map(|account| account.lamports()).collect()
}

// Token balance collection for SPL token accounts
fn collect_token_balances(
    accounts: &[AccountSharedData], 
    account_loader: &AccountLoader
) -> Vec<TokenBalance> {
    accounts.iter()
        .filter(|account| is_token_account(account))
        .filter_map(|account| parse_token_account(account, account_loader))
        .collect()
}
```

## Error Handling and Rollback

### Transaction Failure Recovery

When transactions fail, the SVM implements sophisticated rollback mechanisms:

```rust
// Rollback accounts for failed transactions
pub enum RollbackAccounts {
    // Only fee payer needs rollback
    FeePayerOnly { fee_payer_account: AccountSharedData },
    
    // Nonce and fee payer are the same account
    SameNonceAndFeePayer { nonce: NonceInfo },
    
    // Separate nonce and fee payer accounts
    SeparateNonceAndFeePayer { 
        nonce: NonceInfo, 
        fee_payer_account: AccountSharedData 
    },
}

impl RollbackAccounts {
    // Restore accounts to pre-execution state
    pub fn restore_state(&self, account_loader: &mut AccountLoader) {
        match self {
            Self::FeePayerOnly { fee_payer_account } => {
                // Restore fee payer lamports (credit back rent)
                account_loader.store_account(fee_payer_key, fee_payer_account.clone());
            }
            Self::SameNonceAndFeePayer { nonce } => {
                // Restore both nonce state and fee payer lamports
                account_loader.store_account(nonce.address(), nonce.account().clone());
            }
            Self::SeparateNonceAndFeePayer { nonce, fee_payer_account } => {
                // Restore both accounts independently
                account_loader.store_account(nonce.address(), nonce.account().clone());
                account_loader.store_account(fee_payer_key, fee_payer_account.clone());
            }
        }
    }
}
```

### Error Classification

The SVM provides detailed error classification:

```rust
// Transaction processing errors
pub enum TransactionError {
    // Account-related errors
    AccountNotFound,
    AccountInUse,
    AccountLoadedTwice,
    
    // Program-related errors  
    ProgramAccountNotFound,
    InvalidProgramForExecution,
    
    // Instruction-specific errors
    InstructionError(u8, InstructionError),
    
    // Fee and balance errors
    InsufficientFundsForFee,
    InsufficientFundsForRent,
    
    // Nonce-related errors
    InvalidNonce,
    AdvanceNonceAccount,
}
```

## Performance and Metrics

### Execution Timing

The SVM collects comprehensive timing data:

```rust
pub struct ExecuteTimings {
    // Overall execution timing
    pub execute_accessories: ExecuteAccessoryTimings,
    
    // Per-program timing details
    pub details: ExecuteDetailsTimings,
}

pub struct ExecuteAccessoryTimings {
    pub process_instructions: ExecuteAccessoryTiming,
    pub create_executor: ExecuteAccessoryTiming,
    pub verify_account_changes: ExecuteAccessoryTiming,
    pub update_accounts_data_len: ExecuteAccessoryTiming,
}
```

### Compute Unit Tracking

```rust
// Compute unit consumption tracking
let mut accumulated_consumed_units = 0u64;

for instruction in transaction.message().instructions() {
    let mut instruction_compute_units = 0;
    
    // Execute instruction and track compute usage
    let result = process_instruction(
        instruction_data,
        &instruction_accounts, 
        &mut instruction_compute_units,
    );
    
    accumulated_consumed_units += instruction_compute_units;
    
    // Enforce compute budget limits
    if accumulated_consumed_units > compute_budget.max_units {
        return Err(TransactionError::ExceededComputeBudget);
    }
}
```

## Usage Examples

### Basic Transaction Processing

```rust
use solana_svm::transaction_processor::TransactionBatchProcessor;

// Create transaction processor
let processor = TransactionBatchProcessor::new(
    slot,
    epoch, 
    Arc::new(RwLock::new(program_cache)),
    sysvar_cache,
    builtin_program_ids,
);

// Process transactions
let results = processor.load_and_execute_sanitized_transactions(
    &callback,           // Account loading callback
    &transactions,       // Batch of transactions to process
    &mut check_results,  // Transaction check results
    &environment,        // Runtime environment config
    &config,            // Processing configuration
)?;

// Handle results
for (index, result) in results.processing_results.iter().enumerate() {
    match result {
        Ok(ProcessedTransaction::Executed(executed_tx)) => {
            if executed_tx.execution_details.status.is_ok() {
                println!("Transaction {} succeeded", index);
            } else {
                println!("Transaction {} failed: {:?}", index, 
                        executed_tx.execution_details.status);
            }
        }
        Ok(ProcessedTransaction::FeesOnly(fees_only_tx)) => {
            println!("Transaction {} failed to load: {:?}", index, 
                    fees_only_tx.load_error);
        }
        Err(error) => {
            println!("Transaction {} could not be processed: {:?}", index, error);
        }
    }
}
```

### Custom Callback Implementation

```rust
use solana_svm_callback::TransactionProcessingCallback;

struct MyCallback {
    accounts: HashMap<Pubkey, AccountSharedData>,
}

impl TransactionProcessingCallback for MyCallback {
    fn account_matches_owners(&self, account: &Pubkey, owners: &[Pubkey]) -> Option<usize> {
        self.accounts.get(account)
            .and_then(|account_data| {
                owners.iter().position(|owner| account_data.owner() == owner)
            })
    }

    fn get_account_shared_data(&self, pubkey: &Pubkey) -> Option<AccountSharedData> {
        self.accounts.get(pubkey).cloned()
    }

    fn add_builtin_account(&self, name: &str, program_id: &Pubkey) {
        // Add builtin program to account storage
        let account = create_builtin_account(name);
        self.accounts.insert(*program_id, account);
    }
}
```

## API Reference

### Core Types

#### `TransactionBatchProcessor<FG: ForkGraph>`
Main interface for transaction processing.

**Key Methods:**
- `load_and_execute_sanitized_transactions()` - Process transaction batch
- `get_environments_for_epoch()` - Get runtime environments for epoch

#### `TransactionProcessingCallback`
Trait for providing account data to the SVM.

**Required Methods:**
- `account_matches_owners()` - Check if account matches owner list
- `get_account_shared_data()` - Load account data by pubkey
- `add_builtin_account()` - Register builtin program account

#### Result Types

**`TransactionProcessingResult`**
```rust
pub type TransactionProcessingResult = Result<ProcessedTransaction, TransactionError>;

pub enum ProcessedTransaction {
    Executed(Box<ExecutedTransaction>),    // Transaction executed successfully
    FeesOnly(Box<FeesOnlyTransaction>),   // Transaction failed but fees collected
}
```

**`LoadAndExecuteSanitizedTransactionsOutput`**
```rust
pub struct LoadAndExecuteSanitizedTransactionsOutput {
    pub error_metrics: TransactionErrorMetrics,      // Error statistics
    pub execute_timings: ExecuteTimings,              // Performance metrics  
    pub processing_results: Vec<TransactionProcessingResult>, // Per-transaction results
    pub balance_collector: Option<BalanceCollector>, // Balance change data
}
```

### Configuration Types

#### `TransactionProcessingEnvironment`
Runtime environment configuration.

#### `TransactionProcessingConfig`  
Processing behavior customization.

#### `ExecutionRecordingConfig`
Controls what execution data is recorded.

---

The SVM provides a robust, efficient, and secure foundation for executing Solana transactions across various deployment scenarios, from full validators to specialized rollup implementations.