use crate::core::lookup::{LinkOutcome, LookupTable, LookupTableLevel, RelinkOutcome};
use crate::core::model;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use anyhow::anyhow;
use parking_lot::RwLock;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// The number of levels in the lookup table is determined by the size of the identifier in bits (that is
/// `IDENTIFIER_SIZE_BYTES * 8`).
pub const LOOKUP_TABLE_LEVELS: usize = model::IDENTIFIER_SIZE_BYTES * 8;

/// It is a 2D array of Identity, where the first dimension is the level and the second dimension is the direction.
/// Uses Arc for shallow cloning - cloned instances share the same underlying data.
pub struct ArrayLookupTable {
    inner: Arc<RwLock<InnerArrayLookupTable>>,
}

struct InnerArrayLookupTable {
    left: Vec<Option<Identity>>,
    right: Vec<Option<Identity>>,
}

impl ArrayLookupTable {
    /// Create a new empty LookupTable instance.
    pub fn new() -> ArrayLookupTable {
        ArrayLookupTable {
            inner: Arc::new(RwLock::new(InnerArrayLookupTable {
                left: vec![None; LOOKUP_TABLE_LEVELS],
                right: vec![None; LOOKUP_TABLE_LEVELS],
            })),
        }
    }
}

impl Clone for ArrayLookupTable {
    fn clone(&self) -> Self {
        // Shallow clone: cloned instances share the same underlying data via Arc
        ArrayLookupTable {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Debug for ArrayLookupTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.read();
        writeln!(f, "ArrayLookupTable: {{")?;
        for (i, (l, r)) in inner.left.iter().zip(inner.right.iter()).enumerate() {
            writeln!(f, "Level: {i}, Left: {l:?}, Right: {r:?}")?;
        }
        write!(f, "}}")
    }
}

impl Default for ArrayLookupTable {
    fn default() -> Self {
        Self::new()
    }
}

impl LookupTable for ArrayLookupTable {
    /// Update the entry at the given level and direction.
    fn update_entry(
        &self,
        identity: Identity,
        level: LookupTableLevel,
        direction: Direction,
    ) -> anyhow::Result<()> {
        if level >= LOOKUP_TABLE_LEVELS {
            return Err(anyhow!(
                "position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let mut inner = self.inner.write();

        match direction {
            Direction::Left => {
                inner.left[level] = Some(identity);
            }
            Direction::Right => {
                inner.right[level] = Some(identity);
            }
        }

        // Log the update operation
        tracing::trace!(
            "lookup table entry updated: level {}, direction {}, identifier {}",
            level,
            direction,
            identity.id()
        );
        Ok(())
    }

    /// Remove the entry at the given level and direction, and flips it to None.
    fn remove_entry(&self, level: LookupTableLevel, direction: Direction) -> anyhow::Result<()> {
        if level >= LOOKUP_TABLE_LEVELS {
            return Err(anyhow!(
                "position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let mut inner = self.inner.write();

        // Record the current entry before removing it for logging
        let current_entry = match direction {
            Direction::Left => inner.left[level],
            Direction::Right => inner.right[level],
        };

        match direction {
            Direction::Left => {
                inner.left[level] = None;
            }
            Direction::Right => {
                inner.right[level] = None;
            }
        }

        // Log the remove operation
        tracing::trace!(
            "removed entry at level {} in direction {:?}: {:?}",
            level,
            direction,
            current_entry
        );
        Ok(())
    }

    /// Get the entry at the given level and direction.
    /// Returns None if the entry does not exist.
    /// Returns Some(Identity) if the entry exists.
    /// Returns an error if the level is out of bounds.
    fn get_entry(
        &self,
        level: LookupTableLevel,
        direction: Direction,
    ) -> anyhow::Result<Option<Identity>> {
        if level >= LOOKUP_TABLE_LEVELS {
            return Err(anyhow!(
                "position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let inner = self.inner.read();

        let entry = match direction {
            Direction::Left => inner.left[level],
            Direction::Right => inner.right[level],
        };

        // Log the get operation
        tracing::trace!(
            "get entry at level {} in direction {:?}: {:?}",
            level,
            direction,
            entry
        );

        Ok(entry)
    }

    /// Atomically decides whether `candidate` becomes the neighbor at `(level, direction)`, or
    /// whether the existing entry there already sits strictly between this node and `candidate`
    /// and the request should be forwarded. Runs entirely under a single `inner.write()` guard —
    /// the compare, the decision, and the (conditional) write all happen under one lock
    /// acquisition.
    ///
    /// This method never calls `get_entry`/`update_entry`, whose separately-locked critical
    /// sections could not be composed into one atomic decision: two concurrent callers linking
    /// the same `(level, direction)` slot could both read the same stale entry under their own
    /// `get_entry` call, both independently decide to insert, and both call `update_entry` —
    /// the second silently clobbers the first, with no forwarding ever evaluated against the
    /// true post-first-write state.
    fn try_link(
        &self,
        level: LookupTableLevel,
        direction: Direction,
        candidate: Identity,
    ) -> anyhow::Result<LinkOutcome> {
        if level >= LOOKUP_TABLE_LEVELS {
            return Err(anyhow!(
                "position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let mut inner = self.inner.write();

        let existing = match direction {
            Direction::Left => inner.left[level],
            Direction::Right => inner.right[level],
        };

        // an existing entry sits strictly between this node and the candidate when, for
        // Direction::Right, existing.id() < candidate.id(); for Direction::Left,
        // existing.id() > candidate.id() — it is then closer to the candidate's true position
        // than this node is, so the slot is left untouched and the decision is to forward.
        let outcome = match (existing, direction) {
            (Some(existing), Direction::Right) if existing.id() < candidate.id() => {
                LinkOutcome::Forward(existing)
            }
            (Some(existing), Direction::Left) if existing.id() > candidate.id() => {
                LinkOutcome::Forward(existing)
            }
            _ => {
                match direction {
                    Direction::Left => inner.left[level] = Some(candidate),
                    Direction::Right => inner.right[level] = Some(candidate),
                }
                LinkOutcome::LinkedDirectly
            }
        };

        // Log the try_link decision
        tracing::trace!(
            "try_link decision at level {} in direction {}: candidate {}, outcome {:?}",
            level,
            direction,
            candidate.id(),
            outcome
        );

        Ok(outcome)
    }

    /// Implements [`LookupTable::try_relink`] — see that doc for what `claimant` means and what
    /// each [`RelinkOutcome`] variant represents. Runs entirely under a single `inner.write()`
    /// guard, for the same reason `try_link` does: composing separately-locked
    /// `get_entry`/`update_entry` calls would reopen a race between two concurrent repair probes
    /// for the same slot, each reading the same stale entry and clobbering the other's write
    /// without ever forwarding against the true post-write state.
    fn try_relink(
        &self,
        level: LookupTableLevel,
        direction: Direction,
        claimant: Identity,
    ) -> anyhow::Result<RelinkOutcome> {
        if level >= LOOKUP_TABLE_LEVELS {
            return Err(anyhow!(
                "position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let mut inner = self.inner.write();

        let existing = match direction {
            Direction::Left => inner.left[level],
            Direction::Right => inner.right[level],
        };

        // three-way decision against the single read of `existing` above, all inside this one
        // write-lock critical section: already-equal is a no-op; strictly-between (same
        // per-direction comparison as try_link) forwards; anything else relinks and evicts.
        let outcome = match (existing, direction) {
            (Some(existing), _) if existing == claimant => RelinkOutcome::AlreadyConsistent,
            (Some(existing), Direction::Right) if existing.id() < claimant.id() => {
                RelinkOutcome::Forward(existing)
            }
            (Some(existing), Direction::Left) if existing.id() > claimant.id() => {
                RelinkOutcome::Forward(existing)
            }
            (evicted, _) => {
                match direction {
                    Direction::Left => inner.left[level] = Some(claimant),
                    Direction::Right => inner.right[level] = Some(claimant),
                }
                RelinkOutcome::Relinked { evicted }
            }
        };

        // Log the try_relink decision
        tracing::trace!(
            "try_relink decision at level {} in direction {}: claimant {}, outcome {:?}",
            level,
            direction,
            claimant.id(),
            outcome
        );

        Ok(outcome)
    }

    /// Returns the list of left neighbors at the current node as a vector of tuples containing the level and identity.
    fn left_neighbors(&self) -> anyhow::Result<Vec<(usize, Identity)>> {
        let inner = self.inner.read();

        let mut neighbors = Vec::new();
        for (level, entry) in inner.left.iter().enumerate() {
            if let Some(identity) = entry {
                neighbors.push((level, *identity));
            }
        }
        Ok(neighbors)
    }

    /// Returns the list of right neighbors at the current node as a vector of tuples containing the level and identity.
    fn right_neighbors(&self) -> anyhow::Result<Vec<(usize, Identity)>> {
        let inner = self.inner.read();

        let mut neighbors = Vec::new();
        for (level, entry) in inner.right.iter().enumerate() {
            if let Some(identity) = entry {
                neighbors.push((level, *identity));
            }
        }
        Ok(neighbors)
    }

    fn clone_box(&self) -> Box<dyn LookupTable> {
        Box::new(self.clone())
    }
}
