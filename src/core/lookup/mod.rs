use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;

pub mod array_lookup_table;
mod array_lookup_table_test;

/// LookupTableLevel represents level of a lookup table. entry in the table.
pub type LookupTableLevel = usize;

/// Outcome of a [`LookupTable::try_link`] compare-then-act decision.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    /// the candidate was inserted at the requested `(level, direction)` slot: the slot was
    /// empty, or its previous occupant did not sit strictly between this node and the
    /// candidate on that side, so the candidate is now the entry there.
    LinkedDirectly,
    /// the table was left untouched: the carried identity is the existing entry at
    /// `(level, direction)`, and it sits strictly between this node and the candidate on that
    /// side — it is closer to the candidate's true position, so the link request belongs there
    /// instead.
    Forward(Identity),
}

/// LookupTable is the core view of Skip Graph node towards the network.
pub trait LookupTable: Send + Sync {
    /// Update the entry at the given level and direction.
    fn update_entry(
        &self,
        identity: Identity,
        level: LookupTableLevel,
        direction: Direction,
    ) -> anyhow::Result<()>;

    /// Remove the entry at the given level and direction.
    fn remove_entry(&self, level: LookupTableLevel, direction: Direction) -> anyhow::Result<()>;

    /// Get the entry at the given level and direction.
    /// Returns None if the entry is not present.
    /// Returns Some(Identity) if the entry is present.
    fn get_entry(
        &self,
        level: LookupTableLevel,
        direction: Direction,
    ) -> anyhow::Result<Option<Identity>>;

    /// atomically decides whether `candidate` becomes the neighbor at `(level, direction)`, or
    /// whether an existing neighbor there already sits strictly between this node and
    /// `candidate` on that side and the request should be forwarded to it instead.
    ///
    /// the decision is atomic: inspecting the current entry and, when accepting, inserting
    /// `candidate` happen as one indivisible step with respect to any other concurrent call on
    /// the same `(level, direction)` slot — no caller can observe or race a partial decision.
    ///
    /// The `direction` parameter is receiver-owned, never re-interpreted hop-to-hop: it always names this
    /// node's own slot (`Direction::Right` this node's own right slot, holding neighbors with
    /// larger identifiers; `Direction::Left` its own left slot), never something relative to a
    /// caller or hop. an existing entry sits strictly between this node and `candidate` when,
    /// for `Direction::Right`, `existing.id() < candidate.id()`; for `Direction::Left`,
    /// `existing.id() > candidate.id()`:
    ///
    /// - if it does: this node's own entry at `(level, direction)` is left unchanged, and
    ///   [`LinkOutcome::Forward`] carrying that existing neighbor is returned — the caller
    ///   should retry the link request against that neighbor instead.
    /// - otherwise (the slot is empty, or the existing entry does not sit strictly between):
    ///   `candidate` is inserted into this node's own entry at `(level, direction)`, and
    ///   [`LinkOutcome::LinkedDirectly`] is returned.
    ///
    /// # Errors
    ///
    /// returns an error when `level` is out of bounds.
    fn try_link(
        &self,
        level: LookupTableLevel,
        direction: Direction,
        candidate: Identity,
    ) -> anyhow::Result<LinkOutcome>;

    /// Dynamically compares the lookup table with another for equality.
    fn equal(&self, other: &dyn LookupTable) -> bool;

    /// Returns the list of left neighbors at the current node as a vector of tuples containing the level and identity.
    fn left_neighbors(&self) -> anyhow::Result<Vec<(usize, Identity)>>;

    /// Returns the list of right neighbors at the current node as a vector of tuples containing the level and identity.
    fn right_neighbors(&self) -> anyhow::Result<Vec<(usize, Identity)>>;

    /// Creates a shallow copy of this lookup table.
    ///
    /// Implementations should ensure that cloned instances share the same underlying data
    /// (e.g., using Arc for shared ownership). Changes made through one instance should be
    /// visible in all cloned instances. This is the standard cloning behavior for all
    /// LookupTable implementations.
    fn clone_box(&self) -> Box<dyn LookupTable>;
}

impl PartialEq for dyn LookupTable {
    fn eq(&self, other: &dyn LookupTable) -> bool {
        self.equal(other)
    }
}

impl Clone for Box<dyn LookupTable> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
