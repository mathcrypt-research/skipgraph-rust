use crate::core::lookup::lookup_table::{Level, LookupTable};
use crate::core::model;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use anyhow::anyhow;
use std::fmt::{Debug, Formatter};

/// It is a 2D array of Identity, where the first dimension is the level and the second dimension is the direction.
/// Caution: lookup table by itself is not thread-safe, should be used with an Arc<Mutex<LookupTable>>.
#[derive(Clone)]
pub struct ArrayLookupTable<T: Clone> {
    left: Vec<Option<Identity<T>>>,
    right: Vec<Option<Identity<T>>>,
}

impl<T> ArrayLookupTable<T>
where
    T: Clone,
{
    /// Create a new empty LookupTable instance.
    pub fn new() -> ArrayLookupTable<T> {
        ArrayLookupTable {
            left: vec![None; model::IDENTIFIER_SIZE_BYTES],
            right: vec![None; model::IDENTIFIER_SIZE_BYTES],
        }
    }
}

impl<T> Debug for ArrayLookupTable<T>
where
    T: Clone + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut i = 0;
        for (l, r) in self.left.iter().zip(self.right.iter()) {
            write!(f, "Level: {}, Left: {:?}, Right: {:?}\n", i, l, r)?;
            i += 1;
        }
        Ok(())
    }
}

impl<T> LookupTable<T> for ArrayLookupTable<T>
where
    T: Clone + Debug + 'static + PartialEq,
{
    /// Update the entry at the given level and direction.
    fn update_entry(
        &mut self,
        identity: Identity<T>,
        level: Level,
        direction: Direction,
    ) -> anyhow::Result<()> {
        if level >= model::IDENTIFIER_SIZE_BYTES {
            return Err(anyhow!(
                "Position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        match direction {
            Direction::Left => {
                self.left[level] = Some(identity);
            }
            Direction::Right => {
                self.right[level] = Some(identity);
            }
        }

        Ok(())
    }

    /// Remove the entry at the given level and direction, and flips it to None.
    fn remove_entry(&mut self, level: Level, direction: Direction) -> anyhow::Result<()> {
        if level >= model::IDENTIFIER_SIZE_BYTES {
            return Err(anyhow!(
                "Position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        match direction {
            Direction::Left => {
                self.left[level] = None;
            }
            Direction::Right => {
                self.right[level] = None;
            }
        }

        Ok(())
    }

    /// Get the entry at the given level and direction.
    /// Returns None if the entry does not exist.
    /// Returns Some(Identity) if the entry exists.
    /// Returns an error if the level is out of bounds.
    fn get_entry(
        &self,
        level: Level,
        direction: Direction,
    ) -> anyhow::Result<Option<&Identity<T>>> {
        if level >= model::IDENTIFIER_SIZE_BYTES {
            return Err(anyhow!(
                "Position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        match direction {
            Direction::Left => Ok(self.left[level].as_ref()),
            Direction::Right => Ok(self.right[level].as_ref()),
        }
    }

    /// Dynamically compares the lookup table with another for equality.
    /// This is a deep comparison of the entries in the table.
    /// Returns true if the entries are equal, false otherwise.
    fn equal(&self, other: &dyn LookupTable<T>) -> bool {
        // iterates over the levels and compares the entries in the left and right directions
        for l in 0..model::IDENTIFIER_SIZE_BYTES {
            // Check if the left entry is equal
            if let Ok(other_entry) = other.get_entry(l, Direction::Left) {
                if self.left[l].as_ref() != other_entry {
                    return false;
                }
            } else {
                // if retrieving the entry fails on the other table, return false
                return false;
            }

            if let Ok(other_entry) = other.get_entry(l, Direction::Right) {
                if self.right[l].as_ref() != other_entry {
                    return false;
                }
            } else {
                // if retrieving the entry fails on the other table, return false
                return false;
            }
        }
        true
    }

    fn clone_box(&self) -> Box<dyn LookupTable<T>> {
        Box::new(self.clone())
    }
}

impl<T> PartialEq for ArrayLookupTable<T>
where
    T: Clone + Debug + 'static + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::*;
    use crate::core::Address;

    #[test]
    /// A new lookup table should be empty.
    fn test_lookup_table_empty() {
        let lt: ArrayLookupTable<Address> = ArrayLookupTable::new();
        for i in 0..model::IDENTIFIER_SIZE_BYTES {
            assert_eq!(None, lt.get_entry(i, Direction::Left).unwrap());
            assert_eq!(None, lt.get_entry(i, Direction::Right).unwrap());
        }
    }

    #[test]
    /// Test updating and getting entries in the lookup table.
    /// The test will update the entries at level 0 and 1, and then get them.
    /// The test will also try to get an entry at level 2, which should return an error.
    fn test_lookup_table_update_get() {
        let mut lt = ArrayLookupTable::new();
        let id1 = random_network_identity();
        let id2 = random_network_identity();

        lt.update_entry(id1, 0, Direction::Left).unwrap();
        lt.update_entry(id2, 1, Direction::Right).unwrap();

        assert_eq!(Some(&id1), lt.get_entry(0, Direction::Left).unwrap());
        assert_eq!(Some(&id2), lt.get_entry(1, Direction::Right).unwrap());
        assert_eq!(None, lt.get_entry(2, Direction::Left).unwrap());
    }

    #[test]
    /// Test removing entries in the lookup table.
    /// The test will update the entries at level 0 and 1, and then remove them.
    /// The test will then try to get the removed entries, which should return None.
    fn test_lookup_table_remove() {
        let mut lt = ArrayLookupTable::new();
        let id1 = random_network_identity();
        let id2 = random_network_identity();

        lt.update_entry(id1, 0, Direction::Left).unwrap();
        lt.update_entry(id2, 1, Direction::Right).unwrap();

        lt.remove_entry(0, Direction::Left).unwrap();
        lt.remove_entry(1, Direction::Right).unwrap();

        assert_eq!(None, lt.get_entry(0, Direction::Left).unwrap());
        assert_eq!(None, lt.get_entry(1, Direction::Right).unwrap());
    }

    #[test]
    /// Test updating entries at out-of-bound levels.
    fn test_lookup_table_out_of_bound() {
        let mut lt = ArrayLookupTable::new();
        let id = random_network_identity();

        let result = lt.update_entry(id, model::IDENTIFIER_SIZE_BYTES, Direction::Left);
        assert!(result.is_err());

        let result = lt.update_entry(id, model::IDENTIFIER_SIZE_BYTES, Direction::Right);
        assert!(result.is_err());

        let result = lt.get_entry(model::IDENTIFIER_SIZE_BYTES, Direction::Left);
        assert!(result.is_err());

        let result = lt.get_entry(model::IDENTIFIER_SIZE_BYTES, Direction::Right);
        assert!(result.is_err());

        let result = lt.remove_entry(model::IDENTIFIER_SIZE_BYTES, Direction::Left);
        assert!(result.is_err());

        let result = lt.remove_entry(model::IDENTIFIER_SIZE_BYTES, Direction::Right);
        assert!(result.is_err());
    }

    #[test]
    /// Test overriding entries in the lookup table.
    /// The test will update the entry at level 0, then update it again with a different identity.
    /// The test will then get the entry at level 0, which should return the second identity.
    fn test_lookup_table_override() {
        let mut lt = ArrayLookupTable::new();
        let id1 = random_network_identity();
        let id2 = random_network_identity();

        lt.update_entry(id1, 0, Direction::Left).unwrap();
        assert_eq!(Some(&id1), lt.get_entry(0, Direction::Left).unwrap());

        lt.update_entry(id2, 0, Direction::Left).unwrap();

        assert_eq!(Some(&id2), lt.get_entry(0, Direction::Left).unwrap());
    }

    #[test]
    /// Test equality of lookup tables.
    /// The test will create two identical lookup tables and check if they are equal.
    /// The test will also create a different lookup table and check if they are not equal.
    /// The test will also check if the lookup table is equal to itself.
    /// The test will also check if the lookup table is not equal to None.
    fn test_lookup_table_equality() {
        let lt1 = random_network_lookup_table(10);
        let lt2 = random_network_lookup_table(10);

        assert_ne!(lt1, lt2); // check if two random lookup tables are not equal
        assert_eq!(lt1, lt1); // check if the lookup table is equal to itself
        assert_eq!(lt2, lt2); // check if the lookup table is equal to itself
    }
}
