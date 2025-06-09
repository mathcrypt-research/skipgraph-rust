use crate::core::lookup::lookup_table::{LookupTableLevel, LookupTable};
use crate::core::model;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use anyhow::anyhow;
use std::fmt::{Debug, Formatter};
use std::sync::RwLock;
use tracing::{Level, Span};

/// It is a 2D array of Identity, where the first dimension is the level and the second dimension is the direction.
/// Caution: lookup table by itself is not thread-safe, should be used with an Arc<Mutex<LookupTable>>.
pub struct ArrayLookupTable<T: Clone> {
    inner: RwLock<InnerArrayLookupTable<T>>,
    span: Span
}

struct InnerArrayLookupTable<T> {
    left: Vec<Option<Identity<T>>>,
    right: Vec<Option<Identity<T>>>,
}

impl<T> ArrayLookupTable<T>
where
    T: Clone,
{
    /// Create a new empty LookupTable instance.
    pub fn new(parent_span: &Span) -> ArrayLookupTable<T> {
        let span = tracing::span!(parent: parent_span, Level::INFO, "array_lookup_table");

        ArrayLookupTable {
            inner: RwLock::new(InnerArrayLookupTable {
                left: vec![None; model::IDENTIFIER_SIZE_BYTES],
                right: vec![None; model::IDENTIFIER_SIZE_BYTES],
            }),
            span,
        }
    }
}

impl<T: Clone> Clone for ArrayLookupTable<T> {
    fn clone(&self) -> Self {
        // Create a new instance of ArrayLookupTable with the same data
        let inner = self.inner.read().unwrap();
        ArrayLookupTable {
            inner: RwLock::new(InnerArrayLookupTable {
                left: inner.left.clone(),
                right: inner.right.clone(),
            }),
            span: self.span.clone(),
        }
    }
}

impl<T> Debug for ArrayLookupTable<T>
where
    T: Clone + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let inner = match self.inner.read() {
            Ok(guard) => guard,
            Err(_) => return write!(f, "Failed to acquire read lock on the lookup table"),
        };
        writeln!(f, "ArrayLookupTable: {{")?;
        for (i, (l, r)) in inner.left.iter().zip(inner.right.iter()).enumerate() {
            writeln!(f, "Level: {}, Left: {:?}, Right: {:?}", i, l, r)?;
        }
        write!(f, "}}")
    }
}

impl<T> LookupTable<T> for ArrayLookupTable<T>
where
    T: Clone + Debug + 'static + PartialEq,
{
    /// Update the entry at the given level and direction.
    fn update_entry(
        &self,
        identity: Identity<T>,
        level: LookupTableLevel,
        direction: Direction,
    ) -> anyhow::Result<()> {
        if level >= model::IDENTIFIER_SIZE_BYTES {
            return Err(anyhow!(
                "Position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let mut inner = match self.inner.write() {
            Ok(guard) => guard,
            Err(_) => return Err(anyhow!("Failed to acquire write lock on the lookup table")),
        };

        match direction {
            Direction::Left => {
                inner.left[level] = Some(identity.clone());
            }
            Direction::Right => {
                inner.right[level] = Some(identity.clone());
            }
        }
        
        // Log the update operation
        let _enter = self.span.enter();
        tracing::trace!(
            "Updated entry at level {} in direction {:?} with identity {:?}",
            level,
            direction,
            identity
        );
        Ok(())
    }
    
    /// Remove the entry at the given level and direction, and flips it to None.
    fn remove_entry(&self, level: LookupTableLevel, direction: Direction) -> anyhow::Result<()> {
        if level >= model::IDENTIFIER_SIZE_BYTES {
            return Err(anyhow!(
                "Position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let mut inner = match self.inner.write() {
            Ok(guard) => guard,
            Err(_) => return Err(anyhow!("Failed to acquire write lock on the lookup table")),
        };

        // Record the current entry before removing it for logging
        let current_entry = match direction {
            Direction::Left => inner.left[level].clone(),
            Direction::Right => inner.right[level].clone(),
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
            "Removed entry at level {} in direction {:?}: {:?}",
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
    fn get_entry(&self, level: LookupTableLevel, direction: Direction) -> anyhow::Result<Option<Identity<T>>> {
        if level >= model::IDENTIFIER_SIZE_BYTES {
            return Err(anyhow!(
                "Position is larger than the max lookup table entry number: {}",
                level
            ));
        }

        let inner = match self.inner.read() {
            Ok(guard) => guard,
            Err(_) => return Err(anyhow!("Failed to acquire read lock on the lookup table")),
        };

        let entry = match direction {
            Direction::Left => inner.left[level].clone(),
            Direction::Right => inner.right[level].clone(),
        };
        
        // Log the get operation
        let _enter = self.span.enter();
        tracing::trace!(
            "Get entry at level {} in direction {:?}: {:?}",
            level,
            direction,
            entry.clone()
        );
        
        Ok(entry)
    }

    /// Dynamically compares the lookup table with another for equality.
    /// This is a deep comparison of the entries in the table.
    /// Returns true if the entries are equal, false otherwise.
    fn equal(&self, other: &dyn LookupTable<T>) -> bool {
        // iterates over the levels and compares the entries in the left and right directions
        let inner = match self.inner.read() {
            Ok(guard) => guard,
            Err(err) => panic!("Failed to acquire read lock on the lookup table: {}", err),
        };
        for l in 0..model::IDENTIFIER_SIZE_BYTES {
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
    use std::collections::HashMap;

    #[test]
    /// A new lookup table should be empty.
    fn test_lookup_table_empty() {
        let lt: ArrayLookupTable<Address> = ArrayLookupTable::new(&span_fixture());
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
        let mut lt = ArrayLookupTable::new(&span_fixture());
        let id1 = random_network_identity();
        let id2 = random_network_identity();

        lt.update_entry(id1, 0, Direction::Left).unwrap();
        lt.update_entry(id2, 1, Direction::Right).unwrap();

        assert_eq!(Some(id1), lt.get_entry(0, Direction::Left).unwrap());
        assert_eq!(Some(id2), lt.get_entry(1, Direction::Right).unwrap());
        assert_eq!(None, lt.get_entry(2, Direction::Left).unwrap());
    }

    #[test]
    /// Test removing entries in the lookup table.
    /// The test will update the entries at level 0 and 1, and then remove them.
    /// The test will then try to get the removed entries, which should return None.
    fn test_lookup_table_remove() {
        let mut lt = ArrayLookupTable::new(&span_fixture());
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
        let mut lt = ArrayLookupTable::new(&span_fixture());
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
        let lt = ArrayLookupTable::new(&span_fixture());
        let id1 = random_network_identity();
        let id2 = random_network_identity();

        lt.update_entry(id1, 0, Direction::Left).unwrap();
        assert_eq!(Some(id1), lt.get_entry(0, Direction::Left).unwrap());

        lt.update_entry(id2, 0, Direction::Left).unwrap();

        assert_eq!(Some(id2), lt.get_entry(0, Direction::Left).unwrap());
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

    /// Test concurrent reads from the lookup table.
    /// Creates a lookup table with 20 entries (10 left and 10 right).
    /// Spawns 20 threads to read the entries concurrently.
    /// Each thread reads an entry at a specific level and direction.
    /// Checks if the entry is correct.
    #[test]
    fn test_concurrent_reads() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let lt = Arc::new(ArrayLookupTable::<Address>::new(&span_fixture()));

        // Generate 20 random identities; 10 for left and 10 for right.
        // The i index is the "left" entry at level i + 10 is the "right" entry at level i.
        let levels = 10;
        let identities = random_network_identities(2 * levels);
        
        for i in 0..levels {
            lt.update_entry(identities[i].clone(), i, Direction::Left)
                .unwrap();
            lt.update_entry(identities[i + levels].clone(), i, Direction::Right)
                .unwrap();
        }
        
        // Number of reader threads
        let num_threads = identities.len();
        let barrier = Arc::new(Barrier::new(num_threads)); // to sync thread start
        
        // Spawn threads to read the entries concurrently
        let mut handles = vec![];
        for i in 0..num_threads {
            let lt_ref = lt.clone();
            let barrier_ref = barrier.clone();
            let id = identities[i].clone();
            let handle = thread::spawn(move || {
                barrier_ref.wait(); // wait for all threads to be ready
                let level = i % levels; // alternate between left and right
                let direction = if i < levels {
                    Direction::Left
                } else {
                    Direction::Right
                };
        
                // Read the entry
                let entry = lt_ref.get_entry(level, direction).unwrap();
        
                // Check if the entry is correct
                assert_eq!(entry, Some(id));
            });
        
            handles.push(handle);
        }
        
        // join all threads with a timeout
        let timeout = std::time::Duration::from_millis(100);
        join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();
    }

    /// Test concurrent writes to the lookup table.
    /// Creates a lookup table with 20 entries (10 left and 10 right).
    /// Spawns 20 threads to write the entries concurrently.
    /// Each thread writes an entry at a specific level and direction.
    /// Checks if the entry is correct.
    #[test]
    fn test_concurrent_writes() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        // Generate 20 random identities; 10 for left and 10 for right.
        // The i index is the "left" entry at level i + 10 is the "right" entry at level i.
        let lt = Arc::new(ArrayLookupTable::<Address>::new(&span_fixture()));
        let levels = 10;
        let identities = random_network_identities(2 * levels);

        // Number of writer threads
        let num_threads = identities.len();
        let barrier = Arc::new(Barrier::new(num_threads)); // to sync thread start

        // Spawn threads to write the entries concurrently
        let mut handles = vec![];
        for i in 0..num_threads {
            let lt_ref = lt.clone();
            let barrier_ref = barrier.clone();
            let id = identities[i].clone();
            let level = i % levels; // alternate between left and right
            let direction = if i < levels {
                Direction::Left
            } else {
                Direction::Right
            };

            let handle = thread::spawn(move || {
                barrier_ref.wait(); // wait for all threads to be ready

                // Write the entry
                lt_ref.update_entry(id, level, direction).unwrap();

                // Read the entry back to check if it was written correctly
                let entry = lt_ref.get_entry(level, direction).unwrap();

                // Check if the entry is correct
                assert_eq!(entry, Some(id));
            });

            handles.push(handle);
        }

        // join all threads with a timeout
        let timeout = std::time::Duration::from_millis(100);
        join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();

        // Check if the entries are correct
        for i in 0..levels {
            let left_entry = lt.get_entry(i, Direction::Left).unwrap();
            let right_entry = lt.get_entry(i, Direction::Right).unwrap();
            assert_eq!(left_entry, Some(identities[i].clone()));
            assert_eq!(right_entry, Some(identities[i + levels].clone()));
        }
    }

    /// Test concurrent operations (read, write, remove) on the lookup table.
    /// Creates an empty lookup table.
    /// Spawns multiple threads to perform random operations concurrently.
    /// Each thread performs a random number of operations (read, write, remove) repeatedly each
    /// on a random level and direction.
    ///
    /// This test confines the number of levels to a smaller number to enforce thread contention on
    /// a smaller number of levels, hence have meaningful concurrency on mutually exclusive operations.
    ///
    /// The core idea is to validate the read operation against the last write operation to the same (level, direction).
    /// Note: failure or flaky behavior of this test may indicate a bug in the implementation.
    #[test]
    fn test_randomized_concurrent_operations_with_validation() {
        use rand::Rng;
        use std::sync::{Arc, Barrier, Mutex};
        use std::thread;

        // Shared context is an atomic unit shared between threads,
        // which contains the lookup table and the last write map tracking the last
        // write operation per (level, direction) to validate the read operation.
        // This data structure must be atomic as a read from both must be done atomically as well
        // as a write to both.
        let shared_context = Arc::new(Mutex::new((
            ArrayLookupTable::<Address>::new(&span_fixture()),
            HashMap::<(usize, Direction), Identity<Address>>::new(),
        )));

        let num_threads = 100;
        let ops_per_thread = 1000;
        let barrier = Arc::new(Barrier::new(num_threads));

        let mut handles = vec![];

        for t_id in 0..num_threads {
            let shared_ref = shared_context.clone();
            let barrier_ref = barrier.clone();


            let handle = thread::spawn(move || {
                let mut rng = rand::rng();
                barrier_ref.wait(); // wait for all threads to be ready

                for _ in 0..ops_per_thread {
                    // Randomly selects a level from [0, num_threads / 10] in order to enforce thread
                    // contention a smaller number of levels, hence have meaningful concurrency on mutually
                    // exclusive operations.
                    let level = rng.random_range(0..num_threads / 10);
                    let direction = if rng.random_bool(0.5) {
                        Direction::Left
                    } else {
                        Direction::Right
                    };

                    // Draw a random operation; 0: read, 1: write, 2: remove
                    let op = rng.random_range(0..3);
                    // println!("Thread {}: op: {}, level: {}, direction: {:?}", t_id, op, level, direction);
                    match op {
                        0 => {
                            let (table, last_writes) = &mut *shared_ref.lock().unwrap();
                            let read_val_opt = table.get_entry(level, direction).unwrap();

                            let last_write_opt = last_writes.get(&(level, direction)).cloned();


                            // Validates the read matches the last written value to the same (level, direction).
                            match (read_val_opt, last_write_opt) {
                                (None, None) => { /* no entry, no last write, expected! */ }
                                (Some(ref read_val), Some(ref last_write)) => {
                                    assert_eq!(
                                        read_val, last_write,
                                        "Thread {}: Read value {:?} does not match last write {:?}",
                                        t_id, read_val, last_write
                                    );
                                }
                                (Some(ref read_val), None) => {
                                    panic!(
                                        "Thread {}: Read value {:?} does not match last write None",
                                        t_id, read_val
                                    );
                                }
                                _ => {
                                    panic!(
                                        "Invalid state: read_val_opt: {:?}, last_write_opt: {:?}",
                                        read_val_opt, last_write_opt
                                    );
                                }
                            }
                        }
                        1 => {
                            // write
                            let (table, last_writes) = &mut *shared_ref.lock().unwrap();

                            let id = random_network_identity();
                            if table.update_entry(id.clone(), level, direction).is_ok() {
                                // Update the last write map upon successful write
                                last_writes.insert((level, direction), id);
                            }
                        }
                        2 => {
                            // remove atomically
                            let (table, last_writes) = &mut *shared_ref.lock().unwrap();
                            if table.remove_entry(level, direction).is_ok() {
                                // Remove the last written entry
                                last_writes.remove(&(level, direction));
                            }
                        }
                        _ => panic!("Invalid operation"),
                    }
                }
            });

            handles.push(handle);
        }
        // join all threads with a timeout
        let timeout = std::time::Duration::from_secs(10);
        join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();
    }
}
