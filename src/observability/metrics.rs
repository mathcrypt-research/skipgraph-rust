use crate::core::model::direction::Direction;
use crate::observability::labels::{LevelBucket, MessageType, SearchOutcome};
use std::time::Duration;

/// Records metrics for identifier-search operations.
#[unimock::unimock(api = SearchMetricsMock)]
pub trait SearchMetrics: Send + Sync {
    /// Records a completed search by its outcome, hop count, and duration.
    fn record_search(&self, outcome: SearchOutcome, hops: usize, elapsed: Duration);
}

/// Records metrics for lookup-table mutations.
#[unimock::unimock(api = LookupTableMetricsMock)]
pub trait LookupTableMetrics: Send + Sync {
    /// Records installation of a neighbor at the given level bucket and direction.
    fn record_neighbor_install(&self, level: LevelBucket, direction: Direction);
}

/// Records metrics for network message flow.
#[unimock::unimock(api = NetworkMetricsMock)]
pub trait NetworkMetrics: Send + Sync {
    /// Records that a message of the given type was sent.
    fn record_message_sent(&self, message_type: MessageType);

    /// Records that a message of the given type was received.
    fn record_message_received(&self, message_type: MessageType);
}

/// The full metric surface a node records against, composing the per-subsystem traits.
pub trait NodeMetrics: SearchMetrics + LookupTableMetrics + NetworkMetrics {}

// Backends implement the three sub-traits; `NodeMetrics` follows automatically and is never implemented directly.
impl<T> NodeMetrics for T where T: SearchMetrics + LookupTableMetrics + NetworkMetrics {}
