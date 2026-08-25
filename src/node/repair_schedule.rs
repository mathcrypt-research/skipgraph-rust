use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::Mutex;
#[cfg(test)]
use tokio::sync::Notify;
use tokio::time::Interval;

/// RepairSchedule abstracts "wait for the next backpointer-repair tick" so the repair task
/// never calls `tokio::time::sleep`/`interval` directly. [`TokioRepairSchedule`] is the
/// production implementation; [`ManualRepairSchedule`] is a test double that a test drives
/// explicitly, one tick at a time, with zero wall-clock waiting.
// TODO: remove once RepairSchedule is wired into the repair task.
#[allow(dead_code)]
pub(crate) trait RepairSchedule: Send + Sync {
    /// Waits for the next repair tick.
    fn tick(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// Production `RepairSchedule`, backed by a real `tokio::time::Interval`.
// TODO: remove once RepairSchedule is wired into the repair task.
#[allow(dead_code)]
pub(crate) struct TokioRepairSchedule {
    interval: Mutex<Interval>,
}

impl TokioRepairSchedule {
    /// Creates a schedule that ticks every `period`.
    ///
    /// # Args
    ///
    /// * `period` - the duration between repair ticks.
    ///
    /// Must be called from within a running Tokio runtime, per
    /// `tokio::time::interval`'s own precondition.
    #[allow(dead_code)] // TODO: remove once RepairSchedule is wired into the repair task.
    pub(crate) fn new(period: Duration) -> Self {
        TokioRepairSchedule {
            interval: Mutex::new(tokio::time::interval(period)),
        }
    }
}

impl RepairSchedule for TokioRepairSchedule {
    fn tick(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.interval.lock().await.tick().await;
        })
    }
}

/// Test `RepairSchedule`, driven by an explicit, manually-fired gate:
/// [`ManualRepairSchedule::fire`] completes exactly one pending or future
/// [`RepairSchedule::tick`] call.
#[cfg(test)]
pub(crate) struct ManualRepairSchedule {
    notify: Notify,
}

#[cfg(test)]
impl ManualRepairSchedule {
    pub(crate) fn new() -> Self {
        ManualRepairSchedule {
            notify: Notify::new(),
        }
    }

    /// Fires exactly one repair tick, completing one pending or future `tick()` call.
    pub(crate) fn fire(&self) {
        self.notify.notify_one();
    }
}

#[cfg(test)]
impl RepairSchedule for ManualRepairSchedule {
    fn tick(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.notify.notified())
    }
}
