//! Observability interfaces for Skip Graph nodes.
//!
//! Defines the interface the node code calls to record metric events. It is
//! backend-free (no OpenTelemetry, Prometheus, or other metrics dependency);
//! concrete backends are implemented elsewhere.
//!
//! # Infallibility contract
//!
//! Every metric method returns `()` and must never panic: emitting a metric
//! cannot fail or influence the operation being measured. Implementations
//! swallow their own backend errors.
//!
//! # Cardinality contract
//!
//! Labels are closed enums only ([`SearchOutcome`], [`MessageType`],
//! [`LevelBucket`]), never identifiers or other unbounded values, so the type
//! system makes a high-cardinality label unrepresentable.

mod labels;

#[cfg(test)]
mod labels_test;

pub use labels::{LevelBucket, MessageType, SearchOutcome};
