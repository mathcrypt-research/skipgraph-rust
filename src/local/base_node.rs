use crate::core::model::direction::Direction;
use crate::core::{
    Identifier, IdentifierSearchRequest, IdentifierSearchResult, LookupTable, MembershipVector,
    Node,
};
use std::fmt;
use std::fmt::Formatter;
use std::rc::Rc;

/// LocalNode is a struct that represents a single node in the local implementation of the skip graph.
pub(crate) struct LocalNode {
    id: Identifier,
    mem_vec: MembershipVector,
    lt: Box<dyn LookupTable>,
}

impl Node for LocalNode {
    type Address = Rc<LocalNode>;

    fn get_identifier(&self) -> &Identifier {
        &self.id
    }

    fn get_membership_vector(&self) -> &MembershipVector {
        &self.mem_vec
    }

    fn get_address(&self) -> Self::Address {
        Rc::new(self.clone())
    }

    /// Searches for an identifier in a level-based structure in a specific direction.
    ///
    /// This function attempts to find an identifier from the given `IdentifierSearchRequest`,
    /// by scanning through levels up to the specified level in the request and filtering values
    /// based on the direction. The result is either the best matching identifier or a fallback
    /// identifier if no matches are found.
    ///
    /// # Arguments
    /// - `req`: A reference to an [`IdentifierSearchRequest`] which contains the search criteria,
    ///   including the target identifier, the direction of the search (`Left` or `Right`), and
    ///   the level up to which the search should proceed.
    ///
    /// # Returns
    /// An [`anyhow::Result`] containing:
    /// - `Ok(IdentifierSearchResult)`: The result of the search, including:
    ///   - The target identifier (as given in the `req`),
    ///   - The level at which the closest match was found,
    ///   - The identifier of the closest match (or the current identifier if no close match was found).
    /// - `Err(anyhow::Error)`: An error if there was an issue while accessing a level or retrieving an entry.
    ///
    /// # Behavior
    /// - The function iterates through the levels from `0` to `req.level()`.
    /// - For each level, it retrieves an entry from the lookup table matching the
    ///   direction (`req.direction()`).
    /// - The entries collected are filtered:
    ///   - **Left direction**: Finds the smallest identifier greater than or equal to the target.
    ///   - **Right direction**: Finds the largest identifier less than or equal to the target.
    /// - If a valid match is found, the associated identifier and level are returned. Otherwise, it falls
    ///   back and returns its own identifier at level `0`.
    ///
    /// # Error Handling
    /// - If an error occurs while accessing an entry at any level, it immediately halts execution
    ///   and returns an `anyhow::Error`.
    ///
    /// # Notes
    /// - If no matching identifier is found at any level, the search defaults to returning
    ///   the caller's own identifier at level `0`.
    /// - The method aims to handle both leftward and rightward searches efficiently. To add support
    ///   for other directions or additional filtering, alterations may be required within the
    ///   filtering logic.
    ///
    /// # Returns Fields Explanation (via `IdentifierSearchResult`)
    /// - `target_id`: Copy of the target identifier for traceability purposes.
    /// - `level`: Indicates the level at which the match was found.
    /// - `matched_id`: The identifier of the closest match or fallback identifier.
    fn search_by_id(
        &self,
        req: &IdentifierSearchRequest,
    ) -> anyhow::Result<IdentifierSearchResult> {
        // Collect neighbors from levels <= req.level in req.direction
        let mut candidates = Vec::new();
        for lvl in 0..=req.level() {
            match self.lt.get_entry(lvl, req.direction()) {
                Ok(Some(identity)) => {
                    candidates.push((*identity.id(), lvl));
                }
                Ok(None) => {
                    // No entry found at this level, continue to the next level
                    continue;
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Error while searching by id in level {}: {}",
                        lvl,
                        e
                    ));
                }
            }
        }

        // Filter candidates based on the direction
        let result = match req.direction() {
            Direction::Left => {
                // In the left direction, the result is the smallest identifier that is greater than or equal to the target
                candidates
                    .into_iter()
                    .filter(|(id, _)| id >= req.target())
                    .min_by_key(|(id, _)| *id)
            }
            Direction::Right => {
                // In the right direction, the result is the greatest identifier that is less than or equal to the target
                candidates
                    .into_iter()
                    .filter(|(id, _)| id <= req.target())
                    .max_by_key(|(id, _)| *id)
            }
        };

        match result {
            Some((id, level)) => {
                // If a candidate is found, return it
                Ok(IdentifierSearchResult::new(*req.target(), level, id))
            }
            None => {
                // If no candidates are found, return its own identifier
                Ok(IdentifierSearchResult::new(
                    *req.target(),
                    0,
                    *self.get_identifier(),
                ))
            }
        }
    }

    fn search_by_mem_vec(
        &self,
        _req: &IdentifierSearchRequest,
    ) -> anyhow::Result<IdentifierSearchResult> {
        todo!()
    }

    fn join(&self, _introducer: Self::Address) -> anyhow::Result<()> {
        todo!()
    }
}

/// Implementing PartialEq for LocalNode to compare the id and membership vector.
/// This basically supports == operator for LocalNode.
/// The cardinal assumption is that the id and membership vector are unique for each node.
impl PartialEq for LocalNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.mem_vec == other.mem_vec
        // ignore lt for equality check as comparing trait objects is non-trivial
    }
}

impl fmt::Debug for LocalNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalNode")
            .field("id", &self.id)
            .field("mem_vec", &self.mem_vec)
            .finish()
    }
}

impl Clone for LocalNode {
    fn clone(&self) -> Self {
        LocalNode {
            id: self.id,
            mem_vec: self.mem_vec,
            lt: self.lt.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::identity::Identity;
    use crate::core::testutil::fixtures::{
        random_address, random_identifier, random_identifier_greater_than,
        random_identifier_less_than, random_lookup_table_with_extremes, random_membership_vector,
        span_fixture,
    };
    use crate::core::{ArrayLookupTable, LOOKUP_TABLE_LEVELS};

    #[test]
    fn test_local_node() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let node = LocalNode {
            id,
            mem_vec,
            lt: Box::new(ArrayLookupTable::new(&span_fixture())),
        };
        assert_eq!(node.get_identifier(), &id);
        assert_eq!(node.get_membership_vector(), &mem_vec);
        // TODO: implement get_address for LocalNode
        // assert_eq!(node.get_address(), &node);
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

            let node = LocalNode {
                id: random_identifier(),
                mem_vec: random_membership_vector(),
                lt: Box::new(lt.clone()),
            };

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

            assert_eq!(expected_lvl, actual_result.level());
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

            let node = LocalNode {
                id: random_identifier(),
                mem_vec: random_membership_vector(),
                lt: Box::new(lt.clone()),
            };

            let actual_result = node.search_by_id(&req).unwrap();

            let (expected_lvl, expected_identity) = lt
                .right_neighbors()
                .unwrap()
                .into_iter()
                .filter(|(lvl, id)| *lvl <= req.level() && id.id() <= req.target())
                .max_by_key(|(_, id)| *id.id())
                .unwrap();

            assert_eq!(expected_lvl, actual_result.level());
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
    ///    - Construct a `LocalNode` with the configured lookup table.
    ///    - Create a search request aimed at the left direction.
    ///    - Invoke the `search_by_id` method using the request.
    ///    - Assert that the result matches the node's own identifier, as no better match is expected.
    ///
    /// Test Assertions:
    /// - The resulting level of the search result should be `0`, indicating the search exhausted all levels.
    /// - The resulting identifier should match the local node's identifier.
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



            let node = LocalNode {
                id: random_identifier(),
                mem_vec: random_membership_vector(),
                lt: Box::new(lt.clone()),
            };

            let direction = Direction::Left;
            let req = IdentifierSearchRequest::new(target, lvl, direction);

            let actual_result = node.search_by_id(&req).unwrap();

            assert_eq!(actual_result.level(), 0);
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
    ///    - Construct a `LocalNode` with the configured lookup table.
    ///    - Create a search request aimed at the right direction.
    ///    - Invoke the `search_by_id` method using the request.
    ///    - Assert that the result matches the node's own identifier, as no better match is expected.
    ///
    /// Test Assertions:
    /// - The resulting level of the search result should be `0`, indicating the search exhausted all levels.
    /// - The resulting identifier should match the local node's identifier.
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

            let node = LocalNode {
                id: random_identifier(),
                mem_vec: random_membership_vector(),
                lt: Box::new(lt.clone()),
            };

            let direction = Direction::Right;
            let req = IdentifierSearchRequest::new(target, lvl, direction);

            let actual_result = node.search_by_id(&req).unwrap();

            assert_eq!(actual_result.level(), 0);
            assert_eq!(*actual_result.result(), *node.get_identifier());
        }
    }
    
    /// Tests the `search_by_id` function of the `LocalNode` struct to verify that it properly returns the exact result
    /// when the target identifier exists in the lookup table at the specified level.
    ///
    /// The test performs the following steps:
    /// 1. Creates a random lookup table with a predefined number of levels (`LOOKUP_TABLE_LEVELS`) using helper functions.
    /// 2. Constructs a `LocalNode` instance with a random identifier, membership vector, and the generated lookup table.
    /// 3. Iterates through each level of the lookup table (`LOOKUP_TABLE_LEVELS`) and both `Direction::Left` and `Direction::Right`.
    /// 4. For each level and direction, fetches the expected target identity from the lookup table and constructs an
    ///    `IdentifierSearchRequest` with the target `id`, level, and direction.
    /// 5. Calls `search_by_id` on the `LocalNode` instance with the constructed request.
    /// 6. Verifies that the returned result's level matches the expected level and the node identifier matches the target identifier.
    ///
    /// This test ensures that the `search_by_id` function works correctly in cases where the exact target identifier is found.
    #[test]
    fn test_search_by_id_exact_result() {
        let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
        
        let node = LocalNode {
            id: random_identifier(),
            mem_vec: random_membership_vector(),
            lt: Box::new(lt.clone()),
        };
        
        // This test should ensure that when the exact target is found, it returns the correct level and identifier.
        for lvl in 0..LOOKUP_TABLE_LEVELS {
            for direction in [Direction::Left, Direction::Right] {
                let target_identity = lt.get_entry(lvl, direction).unwrap().unwrap();
                let target = target_identity.id();
                let req = IdentifierSearchRequest::new(*target, lvl, direction);

                let actual_result = node.search_by_id(&req).unwrap();

                assert_eq!(actual_result.level(), lvl);
                assert_eq!(*actual_result.result(), *target);
            }
        }
    }

    // TODO: test that returns an error when the lookup table returns an error during search at any level.
    // TODO: test that when the exact target is found, it returns the correct level and identifier.
    // TODO: concurrent tests for search_by_id to ensure thread safety and correctness under concurrent access.
}
