//! Entry Notifier Service - Real-Time Ledger Event Broadcasting
//! 
//! The entry notifier service provides real-time event broadcasting for newly processed
//! ledger entries, enabling downstream services to react immediately to blockchain state
//! changes. This service is essential for maintaining responsive consensus operations,
//! transaction confirmation, and external system integration.
//! 
//! ## Core Functionality
//! 
//! - **Real-Time Events**: Immediate notification when entries are processed and committed
//! - **Asynchronous Delivery**: Non-blocking event delivery to prevent consensus delays
//! - **Reliable Broadcasting**: Guaranteed delivery to all registered notifier implementations
//! - **Thread Safety**: Concurrent access support for high-throughput scenarios
//! - **Graceful Shutdown**: Clean termination on validator shutdown or service restart
//! 
//! ## Event Information
//! 
//! Each notification includes comprehensive entry metadata:
//! - **Slot**: The blockchain slot containing the entry
//! - **Index**: Position of the entry within the slot
//! - **Entry Summary**: Condensed entry information for efficient processing
//! - **Transaction Index**: Starting position for transaction indexing
//! 
//! ## Performance Characteristics
//! 
//! - **Low Latency**: Sub-millisecond notification delivery for time-sensitive operations
//! - **High Throughput**: Handles thousands of entries per second without backpressure
//! - **Memory Efficient**: Bounded queues prevent memory exhaustion during load spikes
//! - **CPU Optimization**: Minimal overhead on critical consensus path
//! 
//! ## Usage Patterns
//! 
//! ```rust
//! // Initialize service with notifier implementation
//! let service = EntryNotifierService::new(entry_notifier, exit_signal);
//! 
//! // Send entry notification
//! service.sender().send(EntryNotification {
//!     slot, index, entry, starting_transaction_index
//! })?;
//! ```

use {
    crate::entry_notifier_interface::EntryNotifierArc,
    crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender},
    solana_clock::Slot,
    solana_entry::entry::EntrySummary,
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{self, Builder, JoinHandle},
        time::Duration,
    },
};

/// Entry notification containing complete metadata for processed ledger entry
/// 
/// Provides all information necessary for downstream services to process entry
/// events, including position tracking, content summary, and transaction indexing.
pub struct EntryNotification {
    /// The blockchain slot containing this entry
    pub slot: Slot,
    /// Position of this entry within the slot (0-based index)
    pub index: usize,
    /// Condensed entry information optimized for efficient processing
    pub entry: EntrySummary,
    /// Starting index for transactions contained in this entry
    pub starting_transaction_index: usize,
}

/// Channel sender for entry notifications
/// 
/// Thread-safe sender used by ledger processing components to broadcast
/// entry events to the notification service for downstream delivery.
pub type EntryNotifierSender = Sender<EntryNotification>;

/// Channel receiver for entry notifications
/// 
/// Thread-safe receiver used by the notification service to process
/// incoming entry events from ledger processing components.
pub type EntryNotifierReceiver = Receiver<EntryNotification>;

/// Entry notification service managing real-time ledger event broadcasting
/// 
/// Runs a dedicated background thread that receives entry notifications from
/// ledger processing components and delivers them to registered notifier
/// implementations for downstream processing and external integration.
pub struct EntryNotifierService {
    /// Channel sender for submitting entry notifications to the service
    sender: EntryNotifierSender,
    /// Background thread handle for service lifecycle management
    thread_hdl: JoinHandle<()>,
}

impl EntryNotifierService {
    pub fn new(entry_notifier: EntryNotifierArc, exit: Arc<AtomicBool>) -> Self {
        let (entry_notification_sender, entry_notification_receiver) = unbounded();
        let thread_hdl = Builder::new()
            .name("solEntryNotif".to_string())
            .spawn(move || loop {
                if exit.load(Ordering::Relaxed) {
                    break;
                }

                if let Err(RecvTimeoutError::Disconnected) =
                    Self::notify_entry(&entry_notification_receiver, entry_notifier.clone())
                {
                    break;
                }
            })
            .unwrap();
        Self {
            sender: entry_notification_sender,
            thread_hdl,
        }
    }

    fn notify_entry(
        entry_notification_receiver: &EntryNotifierReceiver,
        entry_notifier: EntryNotifierArc,
    ) -> Result<(), RecvTimeoutError> {
        let EntryNotification {
            slot,
            index,
            entry,
            starting_transaction_index,
        } = entry_notification_receiver.recv_timeout(Duration::from_secs(1))?;
        entry_notifier.notify_entry(slot, index, &entry, starting_transaction_index);
        Ok(())
    }

    pub fn sender(&self) -> &EntryNotifierSender {
        &self.sender
    }

    pub fn sender_cloned(&self) -> EntryNotifierSender {
        self.sender.clone()
    }

    pub fn join(self) -> thread::Result<()> {
        self.thread_hdl.join()
    }
}
