#[cfg(test)]
mod tests {
    use crate::core::testutil::fixtures::*;
    use std::collections::HashMap;
    use crate::core::{model, ArrayLookupTable, LookupTable, LOOKUP_TABLE_LEVELS};
    use crate::core::model::direction::Direction;
    use crate::core::model::identity::Identity;

    #[test]
    /// A new lookup table should be empty.
    fn test_lookup_table_empty() {
        let lt: ArrayLookupTable = ArrayLookupTable::new(&span_fixture());
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
        let lt = ArrayLookupTable::new(&span_fixture());
        let id1 = random_identity();
        let id2 = random_identity();

        lt.update_entry(id1.clone(), 0, Direction::Left).unwrap();
        lt.update_entry(id2.clone(), 1, Direction::Right).unwrap();

        assert_eq!(Some(id1), lt.get_entry(0, Direction::Left).unwrap());
        assert_eq!(Some(id2), lt.get_entry(1, Direction::Right).unwrap());
        assert_eq!(None, lt.get_entry(2, Direction::Left).unwrap());
    }

    #[test]
    /// Test removing entries in the lookup table.
    /// The test will update the entries at level 0 and 1, and then remove them.
    /// The test will then try to get the removed entries, which should return None.
    fn test_lookup_table_remove() {
        let lt = ArrayLookupTable::new(&span_fixture());
        let id1 = random_identity();
        let id2 = random_identity();

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
        let lt = ArrayLookupTable::new(&span_fixture());
        let id = random_identity();

        let result = lt.update_entry(id.clone(), LOOKUP_TABLE_LEVELS, Direction::Left);
        assert!(result.is_err());

        let result = lt.update_entry(id, LOOKUP_TABLE_LEVELS, Direction::Right);
        assert!(result.is_err());

        let result = lt.get_entry(LOOKUP_TABLE_LEVELS, Direction::Left);
        assert!(result.is_err());

        let result = lt.get_entry(LOOKUP_TABLE_LEVELS, Direction::Right);
        assert!(result.is_err());

        let result = lt.remove_entry(LOOKUP_TABLE_LEVELS, Direction::Left);
        assert!(result.is_err());

        let result = lt.remove_entry(LOOKUP_TABLE_LEVELS, Direction::Right);
        assert!(result.is_err());
    }

    #[test]
    /// Test overriding entries in the lookup table.
    /// The test will update the entry at level 0, then update it again with a different identity.
    /// The test will then get the entry at level 0, which should return the second identity.
    fn test_lookup_table_override() {
        let lt = ArrayLookupTable::new(&span_fixture());
        let id1 = random_identity();
        let id2 = random_identity();

        lt.update_entry(id1.clone(), 0, Direction::Left).unwrap();
        assert_eq!(Some(id1), lt.get_entry(0, Direction::Left).unwrap());

        lt.update_entry(id2.clone(), 0, Direction::Left).unwrap();

        assert_eq!(Some(id2), lt.get_entry(0, Direction::Left).unwrap());
    }

    #[test]
    /// Test equality of lookup tables.
    /// The test will create two identical lookup tables and check if they are equal.
    /// The test will also create a different lookup table and check if they are not equal.
    /// The test will also check if the lookup table is equal to itself.
    /// The test will also check if the lookup table is not equal to None.
    fn test_lookup_table_equality() {
        let lt1 = random_lookup_table(10);
        let lt2 = random_lookup_table(10);

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

        let lt = Arc::new(ArrayLookupTable::new(&span_fixture()));

        // Generate 20 random identities; 10 for left and 10 for right.
        // The i index is the "left" entry at level i + 10 is the "right" entry at level i.
        let levels = 10;
        let identities = random_identities(2 * levels);

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
        for (i, id) in identities.iter().enumerate().take(num_threads) {
            let lt_ref = lt.clone();
            let barrier_ref = barrier.clone();
            let id = id.clone();
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
        let lt = Arc::new(ArrayLookupTable::new(&span_fixture()));
        let levels = 10;
        let identities = random_identities(2 * levels);

        // Number of writer threads
        let num_threads = identities.len();
        let barrier = Arc::new(Barrier::new(num_threads)); // to sync thread start

        // Spawn threads to write the entries concurrently
        let mut handles = vec![];
        for (i, id) in identities.iter().enumerate().take(num_threads) {
            let lt_ref = lt.clone();
            let barrier_ref = barrier.clone();
            let id = id.clone();
            let level = i % levels; // alternate between left and right
            let direction = if i < levels {
                Direction::Left
            } else {
                Direction::Right
            };

            let handle = thread::spawn(move || {
                barrier_ref.wait(); // wait for all threads to be ready

                // Write the entry
                lt_ref.update_entry(id.clone(), level, direction).unwrap();

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
        use parking_lot::Mutex;
        use std::sync::{Arc, Barrier};
        use std::thread;

        // Shared context is an atomic unit shared between threads,
        // which contains the lookup table and the last write map tracking the last
        // write operation per (level, direction) to validate the read operation.
        // This data structure must be atomic as a read from both must be done atomically as well
        // as a write to both.
        let shared_context = Arc::new(Mutex::new((
            ArrayLookupTable::new(&span_fixture()),
            HashMap::<(usize, Direction), Identity>::new(),
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
                            let (table, last_writes) = &mut *shared_ref.lock();
                            let read_val_opt = table.get_entry(level, direction).unwrap();

                            let last_write_opt = last_writes.get(&(level, direction)).cloned();

                            // Validates the read matches the last written value to the same (level, direction).
                            match (read_val_opt.clone(), last_write_opt.clone()) {
                                (None, None) => { /* no entry, no last write, expected! */ }
                                (Some(ref read_val), Some(ref last_write)) => {
                                    assert_eq!(
                                        read_val, last_write,
                                        "thread {t_id}: read value {read_val:?} does not match last write {last_write:?}"
                                    );
                                }
                                (Some(ref read_val), None) => {
                                    panic!(
                                        "thread {t_id}: read value {read_val:?} does not match last write None"
                                    );
                                }
                                _ => {
                                    panic!(
                                        "invalid state: read_val_opt: {read_val_opt:?}, last_write_opt: {last_write_opt:?}",
                                    );
                                }
                            }
                        }
                        1 => {
                            // write
                            let (table, last_writes) = &mut *shared_ref.lock();

                            let id = random_identity();
                            if table.update_entry(id.clone(), level, direction).is_ok() {
                                // Update the last write map upon successful write
                                last_writes.insert((level, direction), id);
                            }
                        }
                        2 => {
                            // remove atomically
                            let (table, last_writes) = &mut *shared_ref.lock();
                            if table.remove_entry(level, direction).is_ok() {
                                // Remove the last written entry
                                last_writes.remove(&(level, direction));
                            }
                        }
                        _ => panic!("invalid operation"),
                    }
                }
            });

            handles.push(handle);
        }
        // join all threads with a timeout
        let timeout = std::time::Duration::from_secs(10);
        join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();
    }

    /// Tests the retrieval of left and right neighbors from the lookup table.
    #[test]
    fn test_left_and_right_neighbors() {
        let lt = random_lookup_table(LOOKUP_TABLE_LEVELS);

        let rights = lt.right_neighbors().unwrap();
        assert_eq!(rights.len(), LOOKUP_TABLE_LEVELS);
        for (level, identity) in rights.iter() {
            assert_eq!(
                lt.get_entry(*level, Direction::Right).unwrap(),
                Some(identity.clone())
            );
        }

        let lefts = lt.left_neighbors().unwrap();
        assert_eq!(lefts.len(), LOOKUP_TABLE_LEVELS);
        for (level, identity) in lefts.iter() {
            assert_eq!(
                lt.get_entry(*level, Direction::Left).unwrap(),
                Some(identity.clone())
            );
        }
    }

    /// Tests that cloning ArrayLookupTable creates a shallow copy.
    /// Changes made to one instance should be visible in the cloned instance.
    #[test]
    fn test_shallow_clone() {
        let lt1 = ArrayLookupTable::new(&span_fixture());
        let id1 = random_identity();

        // Clone the lookup table
        let lt2 = lt1.clone();

        // Update the original lookup table
        lt1.update_entry(id1.clone(), 0, Direction::Left).unwrap();

        // Verify the cloned lookup table sees the same data
        assert_eq!(lt2.get_entry(0, Direction::Left).unwrap(), Some(id1.clone()));

        // Update through the cloned lookup table
        let id2 = random_identity();
        lt2.update_entry(id2.clone(), 1, Direction::Right).unwrap();

        // Verify the original lookup table sees the change made through the clone
        assert_eq!(lt1.get_entry(1, Direction::Right).unwrap(), Some(id2.clone()));

        // Both instances should be equal since they share the same underlying data
        assert_eq!(lt1, lt2);

        // Verify multiple clones all share the same data
        let lt3 = lt2.clone();
        let id3 = random_identity();
        lt3.update_entry(id3.clone(), 2, Direction::Left).unwrap();

        // All instances should see the new change
        assert_eq!(lt1.get_entry(2, Direction::Left).unwrap(), Some(id3.clone()));
        assert_eq!(lt2.get_entry(2, Direction::Left).unwrap(), Some(id3.clone()));
        assert_eq!(lt3.get_entry(2, Direction::Left).unwrap(), Some(id3.clone()));
    }

    /// Tests that cloning via trait objects (Box<dyn LookupTable>) also creates shallow copies.
    /// This ensures the clone_box method provides the same shallow cloning behavior.
    #[test]
    fn test_trait_object_shallow_clone() {
        let lt1: Box<dyn LookupTable> = Box::new(ArrayLookupTable::new(&span_fixture()));
        let id1 = random_identity();

        // Clone via trait object
        let lt2 = lt1.clone();

        // Update the original lookup table
        lt1.update_entry(id1.clone(), 0, Direction::Left).unwrap();

        // Verify the cloned lookup table sees the same data
        assert_eq!(lt2.get_entry(0, Direction::Left).unwrap(), Some(id1.clone()));

        // Update through the cloned lookup table
        let id2 = random_identity();
        lt2.update_entry(id2.clone(), 1, Direction::Right).unwrap();

        // Verify the original lookup table sees the change made through the clone
        assert_eq!(lt1.get_entry(1, Direction::Right).unwrap(), Some(id2.clone()));

        // Both trait objects should be equal since they share the same underlying data
        assert!(lt1.equal(&*lt2));

        // Test multiple levels of cloning
        let lt3 = lt2.clone();
        let id3 = random_identity();
        lt3.update_entry(id3.clone(), 2, Direction::Left).unwrap();

        // All instances should see the new change
        assert_eq!(lt1.get_entry(2, Direction::Left).unwrap(), Some(id3.clone()));
        assert_eq!(lt2.get_entry(2, Direction::Left).unwrap(), Some(id3.clone()));
        assert_eq!(lt3.get_entry(2, Direction::Left).unwrap(), Some(id3.clone()));
    }
}