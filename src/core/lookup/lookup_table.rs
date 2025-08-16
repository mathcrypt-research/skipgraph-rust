use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;

/// LookupTableLevel represents level of a lookup table. entry in the table.
pub type LookupTableLevel = usize;

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
