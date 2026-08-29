#[cfg(test)]
mod tests {
    use crate::core::model::direction::Direction;
    use crate::core::model::identity::Identity;
    use crate::core::testutil::fixtures::*;
    use crate::core::{
        model, ArrayLookupTable, LinkOutcome, LookupTable, RelinkOutcome, LOOKUP_TABLE_LEVELS,
    };
    use std::collections::HashMap;

    #[test]
    /// A new lookup table should be empty.
    fn test_lookup_table_empty() {
        let lt: ArrayLookupTable = ArrayLookupTable::new();
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
        let lt = ArrayLookupTable::new();
        let id1 = random_identity();
        let id2 = random_identity();

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
        let lt = ArrayLookupTable::new();
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
        let lt = ArrayLookupTable::new();
        let id = random_identity();

        let result = lt.update_entry(id, LOOKUP_TABLE_LEVELS, Direction::Left);
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
        let lt = ArrayLookupTable::new();
        let id1 = random_identity();
        let id2 = random_identity();

        lt.update_entry(id1, 0, Direction::Left).unwrap();
        assert_eq!(Some(id1), lt.get_entry(0, Direction::Left).unwrap());

        lt.update_entry(id2, 0, Direction::Left).unwrap();

        assert_eq!(Some(id2), lt.get_entry(0, Direction::Left).unwrap());
    }

    /// (a) try_link into an empty slot links directly and inserts the candidate, on both sides.
    #[test]
    fn test_try_link_empty_slot_links_directly() {
        let lt = ArrayLookupTable::new();
        let right_candidate = random_identity();
        let left_candidate = random_identity();

        let outcome = lt.try_link(0, Direction::Right, right_candidate).unwrap();
        assert_eq!(outcome, LinkOutcome::LinkedDirectly);
        assert_eq!(
            lt.get_entry(0, Direction::Right).unwrap(),
            Some(right_candidate)
        );

        let outcome = lt.try_link(0, Direction::Left, left_candidate).unwrap();
        assert_eq!(outcome, LinkOutcome::LinkedDirectly);
        assert_eq!(
            lt.get_entry(0, Direction::Left).unwrap(),
            Some(left_candidate)
        );
    }

    /// (b, Right) an existing right neighbor that does NOT sit strictly between self and the
    /// candidate (existing.id() > candidate.id()) is overwritten: try_link links directly and
    /// get_entry afterward reflects the new candidate, not the old neighbor.
    #[test]
    fn test_try_link_existing_not_between_overwrites_right() {
        let lt = ArrayLookupTable::new();
        let candidate_id = random_identifier();
        let candidate = Identity::new(candidate_id, random_membership_vector(), random_address());

        let existing_id = random_identifier_greater_than(&candidate_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Right).unwrap();

        let outcome = lt.try_link(0, Direction::Right, candidate).unwrap();
        assert_eq!(outcome, LinkOutcome::LinkedDirectly);
        assert_eq!(lt.get_entry(0, Direction::Right).unwrap(), Some(candidate));
    }

    /// (b, Left) an existing left neighbor that does NOT sit strictly between self and the
    /// candidate (existing.id() < candidate.id()) is overwritten: try_link links directly and
    /// get_entry afterward reflects the new candidate, not the old neighbor.
    #[test]
    fn test_try_link_existing_not_between_overwrites_left() {
        let lt = ArrayLookupTable::new();
        let candidate_id = random_identifier();
        let candidate = Identity::new(candidate_id, random_membership_vector(), random_address());

        let existing_id = random_identifier_less_than(&candidate_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Left).unwrap();

        let outcome = lt.try_link(0, Direction::Left, candidate).unwrap();
        assert_eq!(outcome, LinkOutcome::LinkedDirectly);
        assert_eq!(lt.get_entry(0, Direction::Left).unwrap(), Some(candidate));
    }

    /// (c, Right) an existing right neighbor that sits strictly between self and the candidate
    /// (existing.id() < candidate.id()) causes try_link to forward instead of linking: the table
    /// is left unchanged, still holding the existing neighbor.
    #[test]
    fn test_try_link_existing_between_forwards_right() {
        let lt = ArrayLookupTable::new();
        let candidate_id = random_identifier();
        let candidate = Identity::new(candidate_id, random_membership_vector(), random_address());

        let existing_id = random_identifier_less_than(&candidate_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Right).unwrap();

        let outcome = lt.try_link(0, Direction::Right, candidate).unwrap();
        assert_eq!(outcome, LinkOutcome::Forward(existing));
        assert_eq!(lt.get_entry(0, Direction::Right).unwrap(), Some(existing));
    }

    /// (c, Left) an existing left neighbor that sits strictly between self and the candidate
    /// (existing.id() > candidate.id()) causes try_link to forward instead of linking: the table
    /// is left unchanged, still holding the existing neighbor.
    #[test]
    fn test_try_link_existing_between_forwards_left() {
        let lt = ArrayLookupTable::new();
        let candidate_id = random_identifier();
        let candidate = Identity::new(candidate_id, random_membership_vector(), random_address());

        let existing_id = random_identifier_greater_than(&candidate_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Left).unwrap();

        let outcome = lt.try_link(0, Direction::Left, candidate).unwrap();
        assert_eq!(outcome, LinkOutcome::Forward(existing));
        assert_eq!(lt.get_entry(0, Direction::Left).unwrap(), Some(existing));
    }

    /// (d) try_link at an out-of-range level returns an error, matching the other lookup-table
    /// accessors' bounds-checking behavior.
    #[test]
    fn test_try_link_out_of_bound_level_errors() {
        let lt = ArrayLookupTable::new();
        let candidate = random_identity();

        let result = lt.try_link(LOOKUP_TABLE_LEVELS, Direction::Right, candidate);
        assert!(result.is_err());

        let result = lt.try_link(LOOKUP_TABLE_LEVELS, Direction::Left, candidate);
        assert!(result.is_err());
    }

    /// (a) try_relink is a no-op returning AlreadyConsistent when the entry already equals the
    /// claimant, on both sides.
    #[test]
    fn test_try_relink_already_consistent() {
        let lt = ArrayLookupTable::new();
        let existing = random_identity();
        lt.update_entry(existing, 0, Direction::Right).unwrap();
        lt.update_entry(existing, 0, Direction::Left).unwrap();
        let outcome = lt.try_relink(0, Direction::Right, existing).unwrap();
        assert_eq!(outcome, RelinkOutcome::AlreadyConsistent);
        assert_eq!(lt.get_entry(0, Direction::Right).unwrap(), Some(existing));
        let outcome = lt.try_relink(0, Direction::Left, existing).unwrap();
        assert_eq!(outcome, RelinkOutcome::AlreadyConsistent);
        assert_eq!(lt.get_entry(0, Direction::Left).unwrap(), Some(existing));
    }

    /// (b, Right) an existing right neighbor strictly between self and the claimant
    /// (existing.id() < claimant.id()) forwards instead of relinking; table unchanged.
    #[test]
    fn test_try_relink_existing_between_forwards_right() {
        let lt = ArrayLookupTable::new();
        let claimant_id = random_identifier();
        let claimant = Identity::new(claimant_id, random_membership_vector(), random_address());
        let existing_id = random_identifier_less_than(&claimant_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Right).unwrap();
        let outcome = lt.try_relink(0, Direction::Right, claimant).unwrap();
        assert_eq!(outcome, RelinkOutcome::Forward(existing));
        assert_eq!(lt.get_entry(0, Direction::Right).unwrap(), Some(existing));
    }

    /// (b, Left) an existing left neighbor strictly between self and the claimant
    /// (existing.id() > claimant.id()) forwards instead of relinking; table unchanged.
    #[test]
    fn test_try_relink_existing_between_forwards_left() {
        let lt = ArrayLookupTable::new();
        let claimant_id = random_identifier();
        let claimant = Identity::new(claimant_id, random_membership_vector(), random_address());
        let existing_id = random_identifier_greater_than(&claimant_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Left).unwrap();
        let outcome = lt.try_relink(0, Direction::Left, claimant).unwrap();
        assert_eq!(outcome, RelinkOutcome::Forward(existing));
        assert_eq!(lt.get_entry(0, Direction::Left).unwrap(), Some(existing));
    }

    /// (c) try_relink into an empty slot relinks with no eviction, on both sides.
    #[test]
    fn test_try_relink_empty_slot_relinks_with_no_eviction() {
        let lt = ArrayLookupTable::new();
        let right_claimant = random_identity();
        let left_claimant = random_identity();
        let outcome = lt.try_relink(0, Direction::Right, right_claimant).unwrap();
        assert_eq!(outcome, RelinkOutcome::Relinked { evicted: None });
        assert_eq!(
            lt.get_entry(0, Direction::Right).unwrap(),
            Some(right_claimant)
        );
        let outcome = lt.try_relink(0, Direction::Left, left_claimant).unwrap();
        assert_eq!(outcome, RelinkOutcome::Relinked { evicted: None });
        assert_eq!(
            lt.get_entry(0, Direction::Left).unwrap(),
            Some(left_claimant)
        );
    }

    /// (d, Right) an existing right neighbor NOT strictly between self and the claimant
    /// (existing.id() > claimant.id()) is evicted; get_entry afterward reflects the claimant.
    #[test]
    fn test_try_relink_existing_not_between_evicts_right() {
        let lt = ArrayLookupTable::new();
        let claimant_id = random_identifier();
        let claimant = Identity::new(claimant_id, random_membership_vector(), random_address());
        let existing_id = random_identifier_greater_than(&claimant_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Right).unwrap();
        let outcome = lt.try_relink(0, Direction::Right, claimant).unwrap();
        assert_eq!(
            outcome,
            RelinkOutcome::Relinked {
                evicted: Some(existing)
            }
        );
        assert_eq!(lt.get_entry(0, Direction::Right).unwrap(), Some(claimant));
    }

    /// (d, Left) an existing left neighbor NOT strictly between self and the claimant
    /// (existing.id() < claimant.id()) is evicted; get_entry afterward reflects the claimant.
    #[test]
    fn test_try_relink_existing_not_between_evicts_left() {
        let lt = ArrayLookupTable::new();
        let claimant_id = random_identifier();
        let claimant = Identity::new(claimant_id, random_membership_vector(), random_address());
        let existing_id = random_identifier_less_than(&claimant_id);
        let existing = Identity::new(existing_id, random_membership_vector(), random_address());
        lt.update_entry(existing, 0, Direction::Left).unwrap();
        let outcome = lt.try_relink(0, Direction::Left, claimant).unwrap();
        assert_eq!(
            outcome,
            RelinkOutcome::Relinked {
                evicted: Some(existing)
            }
        );
        assert_eq!(lt.get_entry(0, Direction::Left).unwrap(), Some(claimant));
    }

    /// (e) try_relink at an out-of-range level returns an error, matching the other
    /// lookup-table accessors' bounds-checking behavior.
    #[test]
    fn test_try_relink_out_of_bound_level_errors() {
        let lt = ArrayLookupTable::new();
        let claimant = random_identity();
        assert!(lt
            .try_relink(LOOKUP_TABLE_LEVELS, Direction::Right, claimant)
            .is_err());
        assert!(lt
            .try_relink(LOOKUP_TABLE_LEVELS, Direction::Left, claimant)
            .is_err());
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

        let lt = Arc::new(ArrayLookupTable::new());

        // Generate 20 random identities; 10 for left and 10 for right.
        // The i index is the "left" entry at level i + 10 is the "right" entry at level i.
        let levels = 10;
        let identities = random_identities(2 * levels);

        for i in 0..levels {
            lt.update_entry(identities[i], i, Direction::Left).unwrap();
            lt.update_entry(identities[i + levels], i, Direction::Right)
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
            let id = *id;
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
        let lt = Arc::new(ArrayLookupTable::new());
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
            let id = *id;
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
            assert_eq!(left_entry, Some(identities[i]));
            assert_eq!(right_entry, Some(identities[i + levels]));
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
        use parking_lot::Mutex;
        use rand::Rng;
        use std::sync::{Arc, Barrier};
        use std::thread;

        // Shared context is an atomic unit shared between threads,
        // which contains the lookup table and the last write map tracking the last
        // write operation per (level, direction) to validate the read operation.
        // This data structure must be atomic as a read from both must be done atomically as well
        // as a write to both.
        let shared_context = Arc::new(Mutex::new((
            ArrayLookupTable::new(),
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
                            match (read_val_opt, last_write_opt) {
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
                            if table.update_entry(id, level, direction).is_ok() {
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
                Some(*identity)
            );
        }

        let lefts = lt.left_neighbors().unwrap();
        assert_eq!(lefts.len(), LOOKUP_TABLE_LEVELS);
        for (level, identity) in lefts.iter() {
            assert_eq!(
                lt.get_entry(*level, Direction::Left).unwrap(),
                Some(*identity)
            );
        }
    }

    /// `max_populated_level` on an empty table returns `None`, distinguishing "nothing
    /// populated" from a populated level 0.
    #[test]
    fn test_max_populated_level_empty_table() {
        let lt = ArrayLookupTable::new();
        assert_eq!(lt.max_populated_level(), None);
    }

    /// `max_populated_level` returns the populated level when only one side of the table has
    /// an entry.
    #[test]
    fn test_max_populated_level_one_side_populated() {
        let lt = ArrayLookupTable::new();
        lt.update_entry(random_identity(), 4, Direction::Left)
            .unwrap();
        assert_eq!(lt.max_populated_level(), Some(4));
    }

    /// `max_populated_level` returns the higher of the two levels when left and right are
    /// populated at different levels.
    #[test]
    fn test_max_populated_level_both_sides_different_levels() {
        let lt = ArrayLookupTable::new();
        lt.update_entry(random_identity(), 2, Direction::Left)
            .unwrap();
        lt.update_entry(random_identity(), 6, Direction::Right)
            .unwrap();
        assert_eq!(lt.max_populated_level(), Some(6));
    }

    /// `max_populated_level` returns `Some(0)`, not `None`, when level 0 is the only populated
    /// entry. An empty table and a table populated only at level 0 must not be conflated.
    #[test]
    fn test_max_populated_level_only_level_zero_populated() {
        let lt = ArrayLookupTable::new();
        lt.update_entry(random_identity(), 0, Direction::Right)
            .unwrap();
        assert_eq!(lt.max_populated_level(), Some(0));
    }

    /// Tests that cloning ArrayLookupTable creates a shallow copy.
    /// Changes made to one instance should be visible in the cloned instance.
    #[test]
    fn test_shallow_clone() {
        let lt1 = ArrayLookupTable::new();
        let id1 = random_identity();

        // Clone the lookup table
        let lt2 = lt1.clone();

        // Update the original lookup table
        lt1.update_entry(id1, 0, Direction::Left).unwrap();

        // Verify the cloned lookup table sees the same data
        assert_eq!(lt2.get_entry(0, Direction::Left).unwrap(), Some(id1));

        // Update through the cloned lookup table
        let id2 = random_identity();
        lt2.update_entry(id2, 1, Direction::Right).unwrap();

        // Verify the original lookup table sees the change made through the clone
        assert_eq!(lt1.get_entry(1, Direction::Right).unwrap(), Some(id2));

        // Both instances see both writes, since they share the same underlying data
        assert_eq!(lt1.get_entry(0, Direction::Left).unwrap(), Some(id1));
        assert_eq!(lt2.get_entry(1, Direction::Right).unwrap(), Some(id2));

        // Verify multiple clones all share the same data
        let lt3 = lt2.clone();
        let id3 = random_identity();
        lt3.update_entry(id3, 2, Direction::Left).unwrap();

        // All instances should see the new change
        assert_eq!(lt1.get_entry(2, Direction::Left).unwrap(), Some(id3));
        assert_eq!(lt2.get_entry(2, Direction::Left).unwrap(), Some(id3));
        assert_eq!(lt3.get_entry(2, Direction::Left).unwrap(), Some(id3));
    }

    /// Tests that cloning via trait objects (Box<dyn LookupTable>) also creates shallow copies.
    /// This ensures the clone_box method provides the same shallow cloning behavior.
    #[test]
    fn test_trait_object_shallow_clone() {
        let lt1: Box<dyn LookupTable> = Box::new(ArrayLookupTable::new());
        let id1 = random_identity();

        // Clone via trait object
        let lt2 = lt1.clone();

        // Update the original lookup table
        lt1.update_entry(id1, 0, Direction::Left).unwrap();

        // Verify the cloned lookup table sees the same data
        assert_eq!(lt2.get_entry(0, Direction::Left).unwrap(), Some(id1));

        // Update through the cloned lookup table
        let id2 = random_identity();
        lt2.update_entry(id2, 1, Direction::Right).unwrap();

        // Verify the original lookup table sees the change made through the clone
        assert_eq!(lt1.get_entry(1, Direction::Right).unwrap(), Some(id2));

        // Both trait objects see both writes, since they share the same underlying data
        assert_eq!(lt1.get_entry(0, Direction::Left).unwrap(), Some(id1));
        assert_eq!(lt2.get_entry(1, Direction::Right).unwrap(), Some(id2));

        // Test multiple levels of cloning
        let lt3 = lt2.clone();
        let id3 = random_identity();
        lt3.update_entry(id3, 2, Direction::Left).unwrap();

        // All instances should see the new change
        assert_eq!(lt1.get_entry(2, Direction::Left).unwrap(), Some(id3));
        assert_eq!(lt2.get_entry(2, Direction::Left).unwrap(), Some(id3));
        assert_eq!(lt3.get_entry(2, Direction::Left).unwrap(), Some(id3));
    }
}
