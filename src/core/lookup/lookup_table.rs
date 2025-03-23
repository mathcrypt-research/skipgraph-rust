use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;

/// LookupTableLevel represents level of a lookup table. entry in the table.
pub type Level = usize;

/// LookupTable is the core view of Skip Graph node towards the network.
pub trait LookupTable {
    /// Update the entry at the given level and direction.
    fn update_entry(
        &mut self,
        identity: Identity,
        level: Level,
        direction: Direction,
    ) -> anyhow::Result<()>;

    /// Remove the entry at the given level and direction.
    fn remove_entry(&mut self, level: Level, direction: Direction) -> anyhow::Result<()>;

    /// Get the entry at the given level and direction.
    /// Returns None if the entry is not present.
    /// Returns Some(Identity) if the entry is present.
    fn get_entry(&self, level: Level, direction: Direction) -> anyhow::Result<Option<&Identity>>;
}
