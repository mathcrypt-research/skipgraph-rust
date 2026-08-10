use crate::core::LookupTableLevel;

/// Outcome of a completed identifier search, used as a metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOutcome {
    /// A matching neighbor was returned by the search.
    Found,
    /// No matching neighbor existed at any level, so the search returned the
    /// caller's own identifier (the Aspnes & Shah own-identifier fallback).
    NotFound,
}

/// Type of a network message, used as a metric label. One variant per event kind.
/// Covers only the search message types today; the rest of `Event` is added with
/// the network instrumentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// A test-only string payload (not used in production).
    TestMessage,
    /// An identifier-search request payload.
    SearchByIdRequest,
    /// An identifier-search response payload.
    SearchByIdResponse,
}

/// The lower bound (inclusive) of the [`LevelBucket::Medium`] range.
const MEDIUM_LEVEL_FLOOR: LookupTableLevel = 16;
/// The lower bound (inclusive) of the [`LevelBucket::High`] range.
const HIGH_LEVEL_FLOOR: LookupTableLevel = 64;
/// The lower bound (inclusive) of the [`LevelBucket::Overflow`] range.
const OVERFLOW_LEVEL_FLOOR: LookupTableLevel = 256;

/// A coarse bucket over a [`LookupTableLevel`], used as a metric label.
///
/// Levels can number in the hundreds; labeling by raw level would explode
/// cardinality, so it is bucketed. The mapping is total, with
/// [`LevelBucket::Overflow`] catching levels at or beyond the expected maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelBucket {
    /// Levels `0..16`.
    Low,
    /// Levels `16..64`.
    Medium,
    /// Levels `64..256`.
    High,
    /// Levels `256` and above (not expected in normal operation).
    Overflow,
}

impl LevelBucket {
    /// Maps a lookup-table level to its bucket. Total over all `usize` values.
    #[must_use]
    pub const fn from_level(level: LookupTableLevel) -> Self {
        if level < MEDIUM_LEVEL_FLOOR {
            LevelBucket::Low
        } else if level < HIGH_LEVEL_FLOOR {
            LevelBucket::Medium
        } else if level < OVERFLOW_LEVEL_FLOOR {
            LevelBucket::High
        } else {
            LevelBucket::Overflow
        }
    }
}

impl From<LookupTableLevel> for LevelBucket {
    fn from(level: LookupTableLevel) -> Self {
        LevelBucket::from_level(level)
    }
}
