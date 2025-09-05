use crate::core::lookup::{LookupTable, LookupTableLevel};
use crate::core::model;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use anyhow::anyhow;
use std::fmt::{Debug, Formatter};
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{Level, Span};

/// The number of levels in the lookup table is determined by the size of the identifier in bits (that is
/// `IDENTIFIER_SIZE_BYTES * 8`).
pub const LOOKUP_TABLE_LEVELS: usize = model::IDENTIFIER_SIZE_BYTES * 8;

/// It is a 2D array of Identity, where the first dimension is the level and the second dimension is the direction.
/// Uses Arc for shallow cloning - cloned instances share the same underlying data.
pub struct ArrayLookupTable {
    inner: Arc<RwLock<InnerArrayLookupTable>>,
    span: Span,
}

struct InnerArrayLookupTable {
    left: Vec<Option<Identity>>,
    right: Vec<Option<Identity>>,
}

impl ArrayLookupTable {
    /// Create a new empty LookupTable instance.
    pub fn new(parent_span: &Span) -> ArrayLookupTable {
        let span = tracing::span!(parent: parent_span, Level::INFO, "array_lookup_table");

        ArrayLookupTable {
            inner: Arc::new(RwLock::new(InnerArrayLookupTable {
                left: vec![None; LOOKUP_TABLE_LEVELS],
                right: vec![None; LOOKUP_TABLE_LEVELS],
            })),
            span,
        }
    }
}

impl Clone for ArrayLookupTable {
    fn clone(&self) -> Self {
        // Shallow clone: cloned instances share the same underlying data via Arc
        ArrayLookupTable {
            inner: Arc::clone(&self.inner),
            span: self.span.clone(),
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
        let _enter = self.span.enter();
        tracing::trace!(
            "updated entry at level {} in direction {:?} with identity {:?}",
            level,
            direction,
            identity
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
        let _enter = self.span.enter();
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
        let _enter = self.span.enter();
        tracing::trace!(
            "get entry at level {} in direction {:?}: {:?}",
            level,
            direction,
            entry
        );

        Ok(entry)
    }

    /// Dynamically compares the lookup table with another for equality.
    /// This is a deep comparison of the entries in the table.
    /// Returns true if the entries are equal, false otherwise.
    fn equal(&self, other: &dyn LookupTable) -> bool {
        // iterates over the levels and compares the entries in the left and right directions
        let inner = self.inner.read();
        for l in 0..LOOKUP_TABLE_LEVELS {
            // Check if the left entry is equal
            if let Ok(other_entry) = other.get_entry(l, Direction::Left) {
                if inner.left[l].as_ref() != other_entry.as_ref() {
                    return false;
                }
            } else {
                // if retrieving the entry fails on the other table, return false
                return false;
            }

            if let Ok(other_entry) = other.get_entry(l, Direction::Right) {
                if inner.right[l].as_ref() != other_entry.as_ref() {
                    return false;
                }
            } else {
                // if retrieving the entry fails on the other table, return false
                return false;
            }
        }
        true
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

impl PartialEq for ArrayLookupTable {
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}


