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

    /// Implements [`LookupTable::try_link`] by delegating to [`LookupTable::try_relink`] and
    /// mapping its outcome as follows.
    ///
    /// - [`RelinkOutcome::Forward`] becomes [`LinkOutcome::Forward`] carrying the same neighbor.
    /// - [`RelinkOutcome::AlreadyConsistent`] and [`RelinkOutcome::Relinked`] both become
    ///   [`LinkOutcome::LinkedDirectly`]. A first-time candidate's caller has no use for the
    ///   eviction detail. A candidate that already holds the slot leaves the table in the same
    ///   state a fresh install would, because [`Identity`] equality is structural over every
    ///   field (id, membership vector, address), so the skipped write and the overwrite are
    ///   indistinguishable.
    fn try_link(
        &self,
        level: LookupTableLevel,
        direction: Direction,
        candidate: Identity,
    ) -> anyhow::Result<LinkOutcome> {
        match self.try_relink(level, direction, candidate)? {
            RelinkOutcome::Forward(existing) => Ok(LinkOutcome::Forward(existing)),
            RelinkOutcome::AlreadyConsistent | RelinkOutcome::Relinked { .. } => {
                Ok(LinkOutcome::LinkedDirectly)
            }
        }
    }

    /// Implements [`LookupTable::try_relink`], and through delegation [`LookupTable::try_link`]
    /// as well. See the trait docs for what `claimant` means and what each [`RelinkOutcome`]
    /// variant represents.
    ///
    /// The compare, the decision, and the conditional write all run under one `inner.write()`
    /// guard. This is the only critical section behind both entry points, so any two concurrent
    /// callers on the same `(level, direction)` slot serialize here, whether they are two link
    /// requests, two repair probes, or one of each. Composing separately-locked
    /// `get_entry`/`update_entry` calls instead would reopen a race between them. Both could read
    /// the same stale entry, both could decide to write, and the second write would silently
    /// clobber the first, with forwarding never evaluated against the true post-first-write
    /// state.
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
        // write-lock critical section. an already-equal entry is a no-op, a strictly-between
        // entry forwards, and anything else relinks and evicts.
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

        tracing::trace!(
            "link decision at level {} in direction {} for identifier {} resolved to {:?}",
            level,
            direction,
            claimant.id(),
            outcome
        );

        Ok(outcome)
    }

    /// Returns the list of left neighbors at the current node as a vector of tuples containing the level and identity.
    fn left_neighbors(&self) -> Vec<(usize, Identity)> {
        let inner = self.inner.read();

        let mut neighbors = Vec::new();
        for (level, entry) in inner.left.iter().enumerate() {
            if let Some(identity) = entry {
                neighbors.push((level, *identity));
            }
        }
        neighbors
    }

    /// Returns the list of right neighbors at the current node as a vector of tuples containing the level and identity.
    fn right_neighbors(&self) -> Vec<(usize, Identity)> {
        let inner = self.inner.read();

        let mut neighbors = Vec::new();
        for (level, entry) in inner.right.iter().enumerate() {
            if let Some(identity) = entry {
                neighbors.push((level, *identity));
            }
        }
        neighbors
    }

    /// Implements [`LookupTable::max_populated_level`] under a single `inner.read()` guard, so
    /// the left and right sides are inspected against the same snapshot of the table.
    fn max_populated_level(&self) -> Option<LookupTableLevel> {
        let inner = self.inner.read();

        (0..LOOKUP_TABLE_LEVELS)
            .rev()
            .find(|&level| inner.left[level].is_some() || inner.right[level].is_some())
    }

    fn clone_box(&self) -> Box<dyn LookupTable> {
        Box::new(self.clone())
    }
}
