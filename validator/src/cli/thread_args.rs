//! Thread Configuration Management for Validator Performance Tuning
//! 
//! This module provides comprehensive thread pool configuration for the Jito-Solana validator.
//! It defines CLI arguments and parsing logic for all thread pools used throughout the validator,
//! enabling fine-tuned performance optimization for different workloads and hardware configurations.
//! 
//! The validator uses multiple specialized thread pools:
//! - Database operations (AccountsDB cleaning, hashing, foreground tasks)
//! - Network operations (TVU receive, retransmit, signature verification)
//! - Storage operations (RocksDB compaction and flushing)
//! - Service threads (IP echo server, replay processing)
//! - Global thread pool (Rayon parallel processing)
//! 
//! Proper thread configuration is critical for validator performance, especially on
//! high-core-count systems where default thread allocation may not be optimal.

use {
    clap::{value_t_or_exit, Arg, ArgMatches},
    solana_accounts_db::{accounts_db, accounts_index},
    solana_clap_utils::{hidden_unless_forced, input_validators::is_within_range},
    solana_rayon_threadlimit::{get_max_thread_count, get_thread_count},
    std::{num::NonZeroUsize, ops::RangeInclusive},
};

/// Default thread count values for all validator thread pools
/// 
/// This struct holds string representations of default thread counts for each
/// validator subsystem. String format is required for CLAP integration to ensure
/// proper lifetime management of default values.
/// 
/// Each field corresponds to a specific validator subsystem that benefits from
/// dedicated thread pool configuration for optimal performance.
pub struct DefaultThreadArgs {
    pub accounts_db_clean_threads: String,
    pub accounts_db_foreground_threads: String,
    pub accounts_db_hash_threads: String,
    pub accounts_index_flush_threads: String,
    pub ip_echo_server_threads: String,
    pub rayon_global_threads: String,
    pub replay_forks_threads: String,
    pub replay_transactions_threads: String,
    pub rocksdb_compaction_threads: String,
    pub rocksdb_flush_threads: String,
    pub tvu_receive_threads: String,
    pub tvu_retransmit_threads: String,
    pub tvu_sigverify_threads: String,
}

impl Default for DefaultThreadArgs {
    fn default() -> Self {
        Self {
            accounts_db_clean_threads: AccountsDbCleanThreadsArg::bounded_default().to_string(),
            accounts_db_foreground_threads: AccountsDbForegroundThreadsArg::bounded_default()
                .to_string(),
            accounts_db_hash_threads: AccountsDbHashThreadsArg::bounded_default().to_string(),
            accounts_index_flush_threads: AccountsIndexFlushThreadsArg::bounded_default()
                .to_string(),
            ip_echo_server_threads: IpEchoServerThreadsArg::bounded_default().to_string(),
            rayon_global_threads: RayonGlobalThreadsArg::bounded_default().to_string(),
            replay_forks_threads: ReplayForksThreadsArg::bounded_default().to_string(),
            replay_transactions_threads: ReplayTransactionsThreadsArg::bounded_default()
                .to_string(),
            rocksdb_compaction_threads: RocksdbCompactionThreadsArg::bounded_default().to_string(),
            rocksdb_flush_threads: RocksdbFlushThreadsArg::bounded_default().to_string(),
            tvu_receive_threads: TvuReceiveThreadsArg::bounded_default().to_string(),
            tvu_retransmit_threads: TvuRetransmitThreadsArg::bounded_default().to_string(),
            tvu_sigverify_threads: TvuShredSigverifyThreadsArg::bounded_default().to_string(),
        }
    }
}

/// Generate CLI arguments for all thread pool configuration options
/// 
/// This function creates the complete set of CLAP arguments for configuring
/// thread pools throughout the validator. Each argument includes validation,
/// help text, and appropriate default values based on system characteristics.
/// 
/// # Arguments
/// * `defaults` - Default values for all thread pool arguments
/// 
/// # Returns
/// Vector of configured CLAP arguments ready for inclusion in the CLI
pub fn thread_args<'a>(defaults: &DefaultThreadArgs) -> Vec<Arg<'_, 'a>> {
    vec![
        new_thread_arg::<AccountsDbCleanThreadsArg>(&defaults.accounts_db_clean_threads),
        new_thread_arg::<AccountsDbForegroundThreadsArg>(&defaults.accounts_db_foreground_threads),
        new_thread_arg::<AccountsDbHashThreadsArg>(&defaults.accounts_db_hash_threads),
        new_thread_arg::<AccountsIndexFlushThreadsArg>(&defaults.accounts_index_flush_threads),
        new_thread_arg::<IpEchoServerThreadsArg>(&defaults.ip_echo_server_threads),
        new_thread_arg::<RayonGlobalThreadsArg>(&defaults.rayon_global_threads),
        new_thread_arg::<ReplayForksThreadsArg>(&defaults.replay_forks_threads),
        new_thread_arg::<ReplayTransactionsThreadsArg>(&defaults.replay_transactions_threads),
        new_thread_arg::<RocksdbCompactionThreadsArg>(&defaults.rocksdb_compaction_threads),
        new_thread_arg::<RocksdbFlushThreadsArg>(&defaults.rocksdb_flush_threads),
        new_thread_arg::<TvuReceiveThreadsArg>(&defaults.tvu_receive_threads),
        new_thread_arg::<TvuRetransmitThreadsArg>(&defaults.tvu_retransmit_threads),
        new_thread_arg::<TvuShredSigverifyThreadsArg>(&defaults.tvu_sigverify_threads),
    ]
}

fn new_thread_arg<'a, T: ThreadArg>(default: &str) -> Arg<'_, 'a> {
    Arg::with_name(T::NAME)
        .long(T::LONG_NAME)
        .takes_value(true)
        .value_name("NUMBER")
        .default_value(default)
        .validator(|num| is_within_range(num, T::range()))
        .hidden(hidden_unless_forced())
        .help(T::HELP)
}

/// Parsed and validated thread configuration for all validator subsystems
/// 
/// This struct contains the final thread pool configuration values after CLI parsing
/// and validation. All values are guaranteed to be valid (non-zero) and within
/// acceptable ranges for the current system.
/// 
/// These values are used throughout validator initialization to configure the
/// various thread pools for optimal performance based on user preferences
/// and system capabilities.
pub struct NumThreadConfig {
    pub accounts_db_clean_threads: NonZeroUsize,
    pub accounts_db_foreground_threads: NonZeroUsize,
    pub accounts_db_hash_threads: NonZeroUsize,
    pub accounts_index_flush_threads: NonZeroUsize,
    pub ip_echo_server_threads: NonZeroUsize,
    pub rayon_global_threads: NonZeroUsize,
    pub replay_forks_threads: NonZeroUsize,
    pub replay_transactions_threads: NonZeroUsize,
    pub rocksdb_compaction_threads: NonZeroUsize,
    pub rocksdb_flush_threads: NonZeroUsize,
    pub tvu_receive_threads: NonZeroUsize,
    pub tvu_retransmit_threads: NonZeroUsize,
    pub tvu_sigverify_threads: NonZeroUsize,
}

/// Parse thread configuration from command-line arguments
/// 
/// Extracts and validates thread pool configuration from parsed CLI arguments.
/// All values are validated to ensure they are within acceptable ranges for
/// the current system and converted to NonZeroUsize for safe usage.
/// 
/// # Arguments
/// * `matches` - Parsed command-line arguments from CLAP
/// 
/// # Returns
/// Validated thread configuration ready for use in validator initialization
/// 
/// # Panics
/// Panics if any thread count argument is invalid (should not happen due to CLAP validation)
pub fn parse_num_threads_args(matches: &ArgMatches) -> NumThreadConfig {
    NumThreadConfig {
        accounts_db_clean_threads: value_t_or_exit!(
            matches,
            AccountsDbCleanThreadsArg::NAME,
            NonZeroUsize
        ),
        accounts_db_foreground_threads: value_t_or_exit!(
            matches,
            AccountsDbForegroundThreadsArg::NAME,
            NonZeroUsize
        ),
        accounts_db_hash_threads: value_t_or_exit!(
            matches,
            AccountsDbHashThreadsArg::NAME,
            NonZeroUsize
        ),
        accounts_index_flush_threads: value_t_or_exit!(
            matches,
            AccountsIndexFlushThreadsArg::NAME,
            NonZeroUsize
        ),
        ip_echo_server_threads: value_t_or_exit!(
            matches,
            IpEchoServerThreadsArg::NAME,
            NonZeroUsize
        ),
        rayon_global_threads: value_t_or_exit!(matches, RayonGlobalThreadsArg::NAME, NonZeroUsize),
        replay_forks_threads: value_t_or_exit!(matches, ReplayForksThreadsArg::NAME, NonZeroUsize),
        replay_transactions_threads: value_t_or_exit!(
            matches,
            ReplayTransactionsThreadsArg::NAME,
            NonZeroUsize
        ),
        rocksdb_compaction_threads: value_t_or_exit!(
            matches,
            RocksdbCompactionThreadsArg::NAME,
            NonZeroUsize
        ),
        rocksdb_flush_threads: value_t_or_exit!(
            matches,
            RocksdbFlushThreadsArg::NAME,
            NonZeroUsize
        ),
        tvu_receive_threads: value_t_or_exit!(matches, TvuReceiveThreadsArg::NAME, NonZeroUsize),
        tvu_retransmit_threads: value_t_or_exit!(
            matches,
            TvuRetransmitThreadsArg::NAME,
            NonZeroUsize
        ),
        tvu_sigverify_threads: value_t_or_exit!(
            matches,
            TvuShredSigverifyThreadsArg::NAME,
            NonZeroUsize
        ),
    }
}

/// Configuration for CLAP arguments that control the number of threads for various functions
trait ThreadArg {
    /// The argument's name
    const NAME: &'static str;
    /// The argument's long name
    const LONG_NAME: &'static str;
    /// The argument's help message
    const HELP: &'static str;

    /// The default number of threads
    fn default() -> usize;
    /// The default number of threads, bounded by Self::max()
    /// This prevents potential CLAP issues on low core count machines where
    /// a fixed value in Self::default() could be greater than Self::max()
    fn bounded_default() -> usize {
        std::cmp::min(Self::default(), Self::max())
    }
    /// The minimum allowed number of threads (inclusive)
    fn min() -> usize {
        1
    }
    /// The maximum allowed number of threads (inclusive)
    fn max() -> usize {
        // By default, no thread pool should scale over the number of the machine's threads
        get_max_thread_count()
    }
    /// The range of allowed number of threads (inclusive on both ends)
    fn range() -> RangeInclusive<usize> {
        RangeInclusive::new(Self::min(), Self::max())
    }
}

struct AccountsDbCleanThreadsArg;
impl ThreadArg for AccountsDbCleanThreadsArg {
    const NAME: &'static str = "accounts_db_clean_threads";
    const LONG_NAME: &'static str = "accounts-db-clean-threads";
    const HELP: &'static str = "Number of threads to use for cleaning AccountsDb";

    fn default() -> usize {
        accounts_db::quarter_thread_count()
    }
}

struct AccountsDbForegroundThreadsArg;
impl ThreadArg for AccountsDbForegroundThreadsArg {
    const NAME: &'static str = "accounts_db_foreground_threads";
    const LONG_NAME: &'static str = "accounts-db-foreground-threads";
    const HELP: &'static str = "Number of threads to use for AccountsDb block processing";

    fn default() -> usize {
        accounts_db::default_num_foreground_threads()
    }
}

struct AccountsDbHashThreadsArg;
impl ThreadArg for AccountsDbHashThreadsArg {
    const NAME: &'static str = "accounts_db_hash_threads";
    const LONG_NAME: &'static str = "accounts-db-hash-threads";
    const HELP: &'static str = "Number of threads to use for background accounts hashing";

    fn default() -> usize {
        accounts_db::default_num_hash_threads().get()
    }
}

struct AccountsIndexFlushThreadsArg;
impl ThreadArg for AccountsIndexFlushThreadsArg {
    const NAME: &'static str = "accounts_index_flush_threads";
    const LONG_NAME: &'static str = "accounts-index-flush-threads";
    const HELP: &'static str = "Number of threads to use for flushing the accounts index";

    fn default() -> usize {
        accounts_index::default_num_flush_threads().get()
    }
}

struct IpEchoServerThreadsArg;
impl ThreadArg for IpEchoServerThreadsArg {
    const NAME: &'static str = "ip_echo_server_threads";
    const LONG_NAME: &'static str = "ip-echo-server-threads";
    const HELP: &'static str = "Number of threads to use for the IP echo server";

    fn default() -> usize {
        solana_net_utils::DEFAULT_IP_ECHO_SERVER_THREADS.get()
    }
    fn min() -> usize {
        solana_net_utils::MINIMUM_IP_ECHO_SERVER_THREADS.get()
    }
}

struct RayonGlobalThreadsArg;
impl ThreadArg for RayonGlobalThreadsArg {
    const NAME: &'static str = "rayon_global_threads";
    const LONG_NAME: &'static str = "rayon-global-threads";
    const HELP: &'static str = "Number of threads to use for the global rayon thread pool";

    fn default() -> usize {
        get_max_thread_count()
    }
}

struct ReplayForksThreadsArg;
impl ThreadArg for ReplayForksThreadsArg {
    const NAME: &'static str = "replay_forks_threads";
    const LONG_NAME: &'static str = "replay-forks-threads";
    const HELP: &'static str = "Number of threads to use for replay of blocks on different forks";

    fn default() -> usize {
        // Default to single threaded fork execution
        1
    }
    fn max() -> usize {
        // Choose a value that is small enough to limit the overhead of having a large thread pool
        // while also being large enough to allow replay of all active forks in most scenarios
        4
    }
}

struct ReplayTransactionsThreadsArg;
impl ThreadArg for ReplayTransactionsThreadsArg {
    const NAME: &'static str = "replay_transactions_threads";
    const LONG_NAME: &'static str = "replay-transactions-threads";
    const HELP: &'static str = "Number of threads to use for transaction replay";

    fn default() -> usize {
        get_max_thread_count()
    }
}

struct RocksdbCompactionThreadsArg;
impl ThreadArg for RocksdbCompactionThreadsArg {
    const NAME: &'static str = "rocksdb_compaction_threads";
    const LONG_NAME: &'static str = "rocksdb-compaction-threads";
    const HELP: &'static str = "Number of threads to use for rocksdb (Blockstore) compactions";

    fn default() -> usize {
        solana_ledger::blockstore::default_num_compaction_threads().get()
    }
}

struct RocksdbFlushThreadsArg;
impl ThreadArg for RocksdbFlushThreadsArg {
    const NAME: &'static str = "rocksdb_flush_threads";
    const LONG_NAME: &'static str = "rocksdb-flush-threads";
    const HELP: &'static str = "Number of threads to use for rocksdb (Blockstore) memtable flushes";

    fn default() -> usize {
        solana_ledger::blockstore::default_num_flush_threads().get()
    }
}

struct TvuReceiveThreadsArg;
impl ThreadArg for TvuReceiveThreadsArg {
    const NAME: &'static str = "tvu_receive_threads";
    const LONG_NAME: &'static str = "tvu-receive-threads";
    const HELP: &'static str =
        "Number of threads (and sockets) to use for receiving shreds on the TVU port";

    fn default() -> usize {
        solana_gossip::cluster_info::DEFAULT_NUM_TVU_RECEIVE_SOCKETS.get()
    }
    fn min() -> usize {
        solana_gossip::cluster_info::MINIMUM_NUM_TVU_RECEIVE_SOCKETS.get()
    }
}

struct TvuRetransmitThreadsArg;
impl ThreadArg for TvuRetransmitThreadsArg {
    const NAME: &'static str = "tvu_retransmit_threads";
    const LONG_NAME: &'static str = "tvu-retransmit-threads";
    const HELP: &'static str = "Number of threads (and sockets) to use for retransmitting shreds";

    fn default() -> usize {
        solana_gossip::cluster_info::DEFAULT_NUM_TVU_RETRANSMIT_SOCKETS.get()
    }

    fn min() -> usize {
        solana_gossip::cluster_info::MINIMUM_NUM_TVU_RETRANSMIT_SOCKETS.get()
    }
}

struct TvuShredSigverifyThreadsArg;
impl ThreadArg for TvuShredSigverifyThreadsArg {
    const NAME: &'static str = "tvu_shred_sigverify_threads";
    const LONG_NAME: &'static str = "tvu-shred-sigverify-threads";
    const HELP: &'static str =
        "Number of threads to use for performing signature verification of received shreds";

    fn default() -> usize {
        get_thread_count()
    }
}
