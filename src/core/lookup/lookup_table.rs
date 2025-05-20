use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use std::fmt::Debug;

/// LookupTableLevel represents level of a lookup table. entry in the table.
pub type Level = usize;

/// LookupTable is the core view of Skip Graph node towards the network.
pub trait LookupTable<T> {
    /// Update the entry at the given level and direction.
    fn update_entry(
        &self,
        identity: Identity<T>,
        level: Level,
        direction: Direction,
    ) -> anyhow::Result<()>;

    /// Remove the entry at the given level and direction.
    fn remove_entry(&self, level: Level, direction: Direction) -> anyhow::Result<()>;

    /// Get the entry at the given level and direction.
    /// Returns None if the entry is not present.
    /// Returns Some(Identity) if the entry is present.
    fn get_entry(&self, level: Level, direction: Direction)
        -> anyhow::Result<Option<Identity<T>>>;

    /// Dynamically compares the lookup table with another for equality.
    fn equal(&self, other: &dyn LookupTable<T>) -> bool;

    fn clone_box(&self) -> Box<dyn LookupTable<T>>;
}

impl<T, U> PartialEq<U> for dyn LookupTable<T>
where
    T: Debug,
    U: LookupTable<T>,
{
    fn eq(&self, other: &U) -> bool {
        self.equal(other)
    }
}

impl<T> Clone for Box<dyn LookupTable<T>> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
