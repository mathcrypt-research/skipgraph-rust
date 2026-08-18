use crate::core::model::search::Nonce;
use crate::core::{IdSearchRes, MaxLevelRes};
use std::collections::HashMap;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Tracks a single outstanding request awaiting a network-delivered reply, keyed by
/// [`Nonce`] in `BaseNode::request_id_map`. One map, one variant per message type, not
/// a map per type, because the lock should protect one logical entity, "requests this
/// node has outstanding". Variants differ in channel primitive because their callers
/// differ in concurrency shape: `search_by_id` stays synchronous (blocking `recv`,
/// unchanged), while `get_max_level` is `async` (a `tokio::sync::oneshot::Receiver`
/// awaited under a timeout).
// TODO: Remove #[allow(dead_code)] once BaseNode is used in production code.
#[allow(dead_code)]
pub(super) enum Waiter {
    /// a pending `search_by_id` call, resolved by a `SearchByIdResponse`.
    Search(SyncSender<IdSearchRes>),
    /// a pending `get_max_level` call, resolved by a `RetMaxLevelOp`.
    MaxLevel(oneshot::Sender<MaxLevelRes>),
}

/// RAII guard that unconditionally removes a nonce's waiter-map entry on drop — ties
/// cleanup to the scope of an in-flight request so success, timeout, and send-failure
/// exits all leave no stale entry.
///
/// A plain `match` with a manual `.remove()` in each non-success branch (as
/// `search_by_id` uses) is not enough here: `get_max_level` is `async`, and an `async`
/// caller can drop the future mid-`.await` (e.g. via `select!` or `JoinHandle::abort`)
/// without running any of that branch code. `Drop` is the only thing Rust still
/// guarantees runs, so it's the only place cleanup can reliably live.
///
/// The removal runs unconditionally, including on the success path — by then the
/// response handler has already removed the entry itself, so this is a harmless no-op
/// (`HashMap::remove` on an absent key just returns `None`). Used only by
/// `get_max_level`; `search_by_id`'s existing manual removals are left as-is.
pub(super) struct WaiterGuard {
    nonce: Nonce,
    map: Arc<Mutex<HashMap<Nonce, Waiter>>>,
}

impl WaiterGuard {
    pub(super) fn new(nonce: Nonce, map: Arc<Mutex<HashMap<Nonce, Waiter>>>) -> Self {
        WaiterGuard { nonce, map }
    }
}

impl Drop for WaiterGuard {
    fn drop(&mut self) {
        // deliberately swallows a poisoned lock instead of `.expect`-panicking, unlike
        // the other lock sites in this project: panicking here could fire mid-unwind
        // (from the very panic that poisoned the lock) and abort the process instead of
        // completing a clean unwind. skipping this best-effort cleanup is safe — it only
        // leaves one stale map entry behind.
        if let Ok(mut map) = self.map.lock() {
            map.remove(&self.nonce);
        }
    }
}
