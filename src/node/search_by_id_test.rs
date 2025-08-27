use super::base_node::BaseNode;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use crate::core::testutil::fixtures::{
    join_all_with_timeout, random_address, random_identifier, random_identifier_greater_than,
    random_identifier_less_than, random_lookup_table_with_extremes, random_membership_vector,
    span_fixture,
};
use crate::core::{
    ArrayLookupTable, Identifier, IdentifierSearchRequest, LookupTable, LookupTableLevel,
    LOOKUP_TABLE_LEVELS,
};

use crate::node::Node;
use anyhow::anyhow;
use rand::Rng;
use std::sync::Arc;
use crate::network::NetworkMock;
use unimock::*;

// TODO: move other tests from base_node.rs here
/// Tests fallback behavior of `search_by_id` when no neighbors exist.
/// Each case mirrors a search on a singleton node as described in the behavior
/// matrix of issue https://github.com/thep2p/skipgraph-rust/issues/22.
#[test]
fn test_search_by_id_singleton_fallback() {
    // Node with identifier 10 and empty lookup table
    let id = Identifier::from_bytes(&[10u8]).unwrap();
    let mem_vec = random_membership_vector();
    let mock_net = Unimock::new((
        NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
        NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
    ));
    let node = BaseNode::new(
        id,
        mem_vec,
        Box::new(ArrayLookupTable::new(&span_fixture())),
        Box::new(mock_net),
    ).expect("Failed to create BaseNode");

    // Left and right searches for identifiers 5 and 15
    let cases = [
        (Identifier::from_bytes(&[5u8]).unwrap(), Direction::Left),
        (Identifier::from_bytes(&[15u8]).unwrap(), Direction::Left),
        (Identifier::from_bytes(&[5u8]).unwrap(), Direction::Right),
        (Identifier::from_bytes(&[15u8]).unwrap(), Direction::Right),
    ];

    for (target, direction) in cases {
        let req = IdentifierSearchRequest::new(target, 3, direction);
        let res = node.search_by_id(&req).expect("search failed");
        // Ensures the search is terminated at the level zero.
        assert_eq!(res.termination_level(), 0);
        // Ensures the search result matches the node's identifier; fallback to self
        assert_eq!(*res.result(), id);
    }
}

/// Test that returns the correct candidate when searching in the left direction,
/// where the smallest identifier greater than or equal to the target should be returned.
#[test]
fn test_search_by_id_found_left_direction() {
    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
        let target = random_identifier();

        // Generate a random identifier greater than the target to ensure we have a candidate
        // Puts the candidate in the left direction at zero level
        let safe_neighbor = random_identifier_greater_than(&target);
        lt.update_entry(
            Identity::new(
                &safe_neighbor,
                &random_membership_vector(),
                random_address(),
            ),
            0,
            Direction::Left,
        )
        .expect("Failed to update entry in lookup table");

        let mock_net = Unimock::new((
            NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
            NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
        ));

        let node = BaseNode::new(
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
            Box::new(mock_net),
        ).expect("Failed to create BaseNode");

        let direction = Direction::Left;
        let req = IdentifierSearchRequest::new(target, lvl, direction);

        let actual_result = node.search_by_id(&req).unwrap();

        let (expected_lvl, expected_identity) = lt
            .left_neighbors()
            .unwrap()
            .into_iter()
            .filter(|(l, id)| *l <= req.level() && id.id() >= req.target())
            .min_by_key(|(_, id)| *id.id())
            .unwrap();

        assert_eq!(expected_lvl, actual_result.termination_level());
        assert_eq!(*expected_identity.id(), *actual_result.result());
    }
}

/// Test that returns the correct candidate when searching in the right direction,
/// where the greatest identifier less than or equal to the target should be returned.
#[test]
fn test_search_by_id_found_right_direction() {
    // Iterate through each level and perform a search
    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
        let target = random_identifier();

        // Generate a random identifier less than the target to ensure we have a candidate
        // Puts the candidate in the right direction at zero level
        let safe_neighbor = random_identifier_less_than(&target);
        lt.update_entry(
            Identity::new(
                &safe_neighbor,
                &random_membership_vector(),
                random_address(),
            ),
            0,
            Direction::Right,
        )
        .expect("Failed to update entry in lookup table");

        let direction = Direction::Right;
        let req = IdentifierSearchRequest::new(target, lvl, direction);

        let mock_net = Unimock::new((
            NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
            NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
        ));
        
        let node = BaseNode::new(
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
            Box::new(mock_net),
        ).expect("Failed to create BaseNode");

        let actual_result = node.search_by_id(&req).unwrap();

        let (expected_lvl, expected_identity) = lt
            .right_neighbors()
            .unwrap()
            .into_iter()
            .filter(|(lvl, id)| *lvl <= req.level() && id.id() <= req.target())
            .max_by_key(|(_, id)| *id.id())
            .unwrap();

        assert_eq!(expected_lvl, actual_result.termination_level());
        assert_eq!(*expected_identity.id(), *actual_result.result());
    }
}

/// Unit test for the `search_by_id` function with the scenario where the target identifier is not found
/// in the left direction within the lookup table.
///
/// This test ensures that when no suitable candidates are found in the left direction, the function returns
/// the node's own address (identifier). The test runs for all levels in the lookup table and validates the
/// behavior.
///
/// Test Steps:
/// 1. Generate a random target identifier.
/// 2. Iteratively test across all levels of the lookup table.
/// 3. For each level:
///    - Populate the left neighbors of the lookup table with entries that all have identifiers
///      less than the target. This guarantees no potential matches in the left direction for the target.
///    - Construct a `BaseNode` with the configured lookup table.
///    - Create a search request aimed at the left direction.
///    - Invoke the `search_by_id` method using the request.
///    - Assert that the result matches the node's own identifier, as no better match is expected.
///
/// Test Assertions:
/// - The resulting level of the search result should be `0`, indicating the search exhausted all levels.
/// - The resulting identifier should match the base node's identifier.
#[test]
fn test_search_by_id_not_found_left_direction() {
    let target = random_identifier();

    // Test that returns the node's own address when no candidates are found matching the target in the left direction.
    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = ArrayLookupTable::new(&span_fixture());

        // Populate the left neighbors of the lookup table with entries that are all less than the target
        // This ensures that no candidates are found in the left direction
        for lvl in 0..LOOKUP_TABLE_LEVELS {
            lt.update_entry(
                Identity::new(
                    &random_identifier_less_than(&target),
                    &random_membership_vector(),
                    random_address(),
                ),
                lvl,
                Direction::Left,
            )
            .expect("Failed to update entry in lookup table");
        }

        let mock_net = Unimock::new((
            NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
            NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
        ));
        
        let node = BaseNode::new(
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
            Box::new(mock_net),
        ).expect("Failed to create BaseNode");

        let direction = Direction::Left;
        let req = IdentifierSearchRequest::new(target, lvl, direction);

        let actual_result = node.search_by_id(&req).unwrap();

        assert_eq!(actual_result.termination_level(), 0);
        assert_eq!(*actual_result.result(), *node.get_identifier());
    }
}

/// Unit test for the `search_by_id` function with the scenario where the target identifier is not found
/// in the right direction within the lookup table.
///
/// This test ensures that when no suitable candidates are found in the right direction, the function returns
/// the node's own address (identifier). The test runs for all levels in the lookup table and validates the
/// behavior.
///
/// Test Steps:
/// 1. Generate a random target identifier.
/// 2. Iteratively test across all levels of the lookup table.
/// 3. For each level:
///    - Populate the right neighbors of the lookup table with entries that all have identifiers
///      less than the target. This guarantees no potential matches in the right direction for the target.
///    - Construct a `BaseNode` with the configured lookup table.
///    - Create a search request aimed at the right direction.
///    - Invoke the `search_by_id` method using the request.
///    - Assert that the result matches the node's own identifier, as no better match is expected.
///
/// Test Assertions:
/// - The resulting level of the search result should be `0`, indicating the search exhausted all levels.
/// - The resulting identifier should match the base node's identifier.
#[test]
fn test_search_by_id_not_found_right_direction() {
    let target = random_identifier();

    // Test that returns the node's own address when no candidates are found matching the target in the right direction.
    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = ArrayLookupTable::new(&span_fixture());

        // Populate the right neighbors of the lookup table with entries that are all greater than the target
        // This ensures that no candidates are found in the right direction
        for lvl in 0..LOOKUP_TABLE_LEVELS {
            lt.update_entry(
                Identity::new(
                    &random_identifier_greater_than(&target),
                    &random_membership_vector(),
                    random_address(),
                ),
                lvl,
                Direction::Right,
            )
            .expect("Failed to update entry in lookup table");
        }

        let mock_net = Unimock::new((
            NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
            NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
        ));
        
        let node = BaseNode::new(
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
            Box::new(mock_net),
        ).expect("Failed to create BaseNode");

        let direction = Direction::Right;
        let req = IdentifierSearchRequest::new(target, lvl, direction);

        let actual_result = node.search_by_id(&req).unwrap();

        assert_eq!(actual_result.termination_level(), 0);
        assert_eq!(*actual_result.result(), *node.get_identifier());
    }
}

/// Tests the `search_by_id` function of the `BaseNode` struct to verify that it properly returns the exact result
/// when the target identifier exists in the lookup table at the specified level.
///
/// The test performs the following steps:
/// 1. Creates a random lookup table with a predefined number of levels (`LOOKUP_TABLE_LEVELS`) using helper functions.
/// 2. Constructs a `BaseNode` instance with a random identifier, membership vector, and the generated lookup table.
/// 3. Iterates through each level of the lookup table (`LOOKUP_TABLE_LEVELS`) and both `Direction::Left` and `Direction::Right`.
/// 4. For each level and direction, fetches the expected target identity from the lookup table and constructs an
///    `IdentifierSearchRequest` with the target `id`, level, and direction.
/// 5. Calls `search_by_id` on the `BaseNode` instance with the constructed request.
/// 6. Verifies that the returned result's level matches the expected level and the node identifier matches the target identifier.
///
/// This test ensures that the `search_by_id` function works correctly in cases where the exact target identifier is found.
#[test]
fn test_search_by_id_exact_result() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);

    let mock_net = Unimock::new((
        NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
        NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
    ));
    
    let node = BaseNode::new(
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
        Box::new(mock_net),
    ).expect("Failed to create BaseNode");

    // This test should ensure that when the exact target is found, it returns the correct level and identifier.
    for lvl in 0..LOOKUP_TABLE_LEVELS {
        for direction in [Direction::Left, Direction::Right] {
            let target_identity = lt.get_entry(lvl, direction).unwrap().unwrap();
            let target = target_identity.id();
            let req = IdentifierSearchRequest::new(*target, lvl, direction);

            let actual_result = node.search_by_id(&req).unwrap();

            assert_eq!(actual_result.termination_level(), lvl);
            assert_eq!(*actual_result.result(), *target);
        }
    }
}

/// Tests the `search_by_id` method of a `BaseNode` under concurrent conditions where multiple
/// threads perform searches in the left direction (`Direction::Left`) simultaneously.
///
/// The test:
/// - Creates a `BaseNode` with a random identifier, random membership vector,
///   and a lookup table (`RandomLookupTable`).
/// - Randomly generates a target identifier to search for.
/// - Spawns 20 threads that conduct searches concurrently from the node.
///
/// ### Test Specific Logic:
/// - Each thread constructs a search request targeting the same identifier and executes the `search_by_id` method.
/// - The expected search result is derived by finding the closest matching identifier from the
///   left neighbors in the lookup table (`lt`) that meets the search criteria (e.g., level,
///   target identifier comparison).
/// - If no valid neighbor is found, it expects the result to default to the `BaseNode`'s own identifier.
///
/// ### Assertions:
/// - If a valid neighbor exists, the search output should match both the level and identifier.
/// - If no valid neighbor exists, the search result should match the node's own identifier at level 0.
#[test]
fn test_search_by_id_concurrent_found_left_direction() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let target = random_identifier();

    // Set up a mock network with shallow cloning and processor registration to simulate concurrent network interactions.
    let mock_net = Unimock::new((
        NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
        NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
    ));
    
    let node = BaseNode::new(
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
        Box::new(mock_net),
    ).expect("Failed to create BaseNode");

    // Ensure the target is not the same as the node's identifier
    assert_ne!(&target, node.get_identifier());

    // Spawn 20 threads to perform concurrent searches
    let num_threads = 20;
    let barrier = Arc::new(std::sync::Barrier::new(num_threads + 1)); // +1 for the main thread
    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for _ in 0..num_threads {
        let handle_barrier = barrier.clone();
        let node_ref = node.clone();
        let lt_clone = lt.clone();
        let handle = std::thread::spawn(move || {
            // Wait for all threads to be ready
            handle_barrier.wait();

            // Pick a random level for the search
            let lvl = rand::rng().random_range(0..LOOKUP_TABLE_LEVELS);

            // Perform the search in the left direction
            let req = IdentifierSearchRequest::new(target, lvl, Direction::Left);
            let actual_result = node_ref.search_by_id(&req).unwrap();

            let expected_result = lt_clone
                .left_neighbors()
                .unwrap()
                .into_iter()
                .filter(|(l, id)| *l <= req.level() && id.id() >= req.target())
                .min_by_key(|(_, id)| *id.id());

            match expected_result {
                Some((expected_lvl, expected_identity)) => {
                    assert_eq!(expected_lvl, actual_result.termination_level());
                    assert_eq!(*expected_identity.id(), *actual_result.result());
                }
                None => {
                    // If no expected result, it should return its own identifier
                    assert_eq!(actual_result.termination_level(), 0);
                    assert_eq!(*actual_result.result(), *node_ref.get_identifier());
                }
            }
        });
        handles.push(handle);
    }

    // Ensures all threads are ready to run before the main thread tries to join them
    // avoiding a situation where the main thread tries to join thread that haven't started yet.
    barrier.wait();
    let timeout = std::time::Duration::from_millis(1000);
    join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();
}

/// Tests the `search_by_id` method of a `BaseNode` under concurrent conditions where multiple
/// threads perform searches in the right direction (`Direction::Right`) simultaneously.
///
/// The test:
/// - Creates a `BaseNode` with a random identifier, random membership vector,
///   and a lookup table (`RandomLookupTable`).
/// - Randomly generates a target identifier to search for.
/// - Spawns 20 threads that conduct searches concurrently from the node.
///
/// ### Test Specific Logic:
/// - Each thread constructs a search request targeting the same identifier and executes the `search_by_id` method.
/// - The expected search result is derived by finding the closest matching identifier from the
///   right neighbors in the lookup table (`lt`) that meets the search criteria (e.g., level,
///   target identifier comparison).
/// - If no valid neighbor is found, it expects the result to default to the `BaseNode`'s own identifier.
///
/// ### Assertions:
/// - If a valid neighbor exists, the search output should match both the level and identifier.
/// - If no valid neighbor exists, the search result should match the node's own identifier at level 0.
#[test]
fn test_search_by_id_concurrent_right_direction() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let target = random_identifier();

    let mock_net = Unimock::new((
        NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
        NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
    ));
    
    let node = BaseNode::new(
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
        Box::new(mock_net),
    ).expect("Failed to create BaseNode");

    // Ensure the target is not the same as the node's identifier
    assert_ne!(&target, node.get_identifier());

    // Spawn 20 threads to perform concurrent searches
    let num_threads = 20;
    let barrier = Arc::new(std::sync::Barrier::new(num_threads + 1)); // +1 for the main thread
    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for _ in 0..num_threads {
        let handle_barrier = barrier.clone();
        let node_ref = node.clone();
        let lt_clone = lt.clone();
        let handle = std::thread::spawn(move || {
            // Wait for all threads to be ready
            handle_barrier.wait();

            // Pick a random level for the search
            let lvl = rand::rng().random_range(0..LOOKUP_TABLE_LEVELS);

            // Perform the search in the right direction
            let req = IdentifierSearchRequest::new(target, lvl, Direction::Right);
            let actual_result = node_ref.search_by_id(&req).unwrap();

            let expected_result = lt_clone
                .right_neighbors()
                .unwrap()
                .into_iter()
                .filter(|(l, id)| *l <= req.level() && id.id() <= req.target())
                .max_by_key(|(_, id)| *id.id());

            match expected_result {
                Some((expected_lvl, expected_identity)) => {
                    assert_eq!(expected_lvl, actual_result.termination_level());
                    assert_eq!(*expected_identity.id(), *actual_result.result());
                }
                None => {
                    // If no expected result, it should return its own identifier
                    assert_eq!(actual_result.termination_level(), 0);
                    assert_eq!(*actual_result.result(), *node_ref.get_identifier());
                }
            }
        });
        handles.push(handle);
    }
    // Ensures all threads are ready to run before the main thread tries to join them
    // avoiding a situation where the main thread tries to join thread that haven't started yet.
    barrier.wait();
    let timeout = std::time::Duration::from_millis(1000);
    join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();
}

/// Test that verifies error handling when the lookup table returns an error during search.
///
/// This test creates a mock lookup table that returns an error when queried at a specific level.
/// It then verifies that the `search_by_id` method properly propagates this error upward rather
/// than silently failing or returning an unexpected result.
#[test]
fn test_search_by_id_error_propagation() {
    // Create a mock lookup table that returns an error for specific lookup operations
    struct MockErrorLookupTable;

    impl Clone for MockErrorLookupTable {
        fn clone(&self) -> Self {
            MockErrorLookupTable
        }
    }

    impl LookupTable for MockErrorLookupTable {
        fn update_entry(
            &self,
            _identity: Identity,
            _level: usize,
            _direction: Direction,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn remove_entry(&self, _: LookupTableLevel, _: Direction) -> anyhow::Result<()> {
            todo!()
        }

        fn get_entry(&self, _: usize, _: Direction) -> anyhow::Result<Option<Identity>> {
            Err(anyhow!("Simulated lookup table error"))
        }

        fn equal(&self, _: &dyn LookupTable) -> bool {
            todo!()
        }

        fn left_neighbors(&self) -> anyhow::Result<Vec<(usize, Identity)>> {
            Ok(Vec::new())
        }

        fn right_neighbors(&self) -> anyhow::Result<Vec<(usize, Identity)>> {
            Ok(Vec::new())
        }

        fn clone_box(&self) -> Box<dyn LookupTable> {
            Box::new(self.clone())
        }
    }

    let mock_net = Unimock::new((
        NetworkMock::register_processor.each_call(matching!(_)).answers(&|_, _| Ok(())),
        NetworkMock::clone_box.each_call(matching!()).answers(&|mock| Box::new(mock.clone())),
    ));

    // Create a base node with the mock error lookup table
    let node = BaseNode::new(
        random_identifier(),
        random_membership_vector(),
        Box::new(MockErrorLookupTable),
        Box::new(mock_net),
    ).expect("Failed to create BaseNode");

    // Create a random search request (any search request will return an error as
    // the mock lookup table is designed to fail)
    let req = IdentifierSearchRequest::new(random_identifier(), 3, Direction::Left);

    // Execute the search and verify that an error is returned
    let result = node.search_by_id(&req);

    // The search should fail with an error message that includes our simulated error
    assert!(
        result.is_err(),
        "Expected an error but got a success result"
    );

    // Check that the error message contains the expected text ("Error while searching by id in level")
    // This error message is constructed in the search_by_id method
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("Error while searching by id in level"),
        "Error message '{error_msg}' doesn't contain expected text"
    );

    // Additionally, check that the error message contains the simulated lookup table error ("Simulated lookup table error")
    // This ensures that the error from the lookup table is propagated correctly
    assert!(
        error_msg.contains("Simulated lookup table error"),
        "Error message '{error_msg}' doesn't contain expected text"
    );
}
