use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use crate::core::testutil::fixtures::{
    join_all_with_timeout, random_address, random_identifier, random_identifier_greater_than,
    random_identifier_less_than, random_lookup_table_with_extremes, random_membership_vector,
    span_fixture,
};
use crate::core::{
    ArrayLookupTable, IdSearchReq, Identifier, LookupTable, LookupTableLevel, LOOKUP_TABLE_LEVELS,
};
use crate::node::core::{BaseCore, Core};
use anyhow::anyhow;
use rand::Rng;
use std::sync::Arc;

fn make_core(id: Identifier, lt: Box<dyn LookupTable>) -> BaseCore {
    BaseCore::new(span_fixture(), id, random_membership_vector(), lt)
}

/// Verifies `search_by_id` returns the core's own identifier when the lookup
/// table is empty.
#[test]
fn test_search_by_id_singleton_fallback() {
    let origin_id = Identifier::from_bytes(&[10u8]).unwrap();
    let core = make_core(origin_id, Box::new(ArrayLookupTable::new()));

    let cases = [
        (Identifier::from_bytes(&[5u8]).unwrap(), Direction::Left),
        (Identifier::from_bytes(&[15u8]).unwrap(), Direction::Left),
        (Identifier::from_bytes(&[5u8]).unwrap(), Direction::Right),
        (Identifier::from_bytes(&[15u8]).unwrap(), Direction::Right),
    ];

    for (target, direction) in cases {
        let req = IdSearchReq::new(origin_id, target, 3, direction);
        let res = core.search_by_id(&req).expect("search failed");
        assert_eq!(res.termination_level(), 0);
        assert_eq!(*res.result(), origin_id);
    }
}

/// Verifies left-direction search returns the smallest neighbor with identifier >= target.
#[test]
fn test_search_by_id_found_left_direction() {
    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
        let target = random_identifier();

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
        .expect("failed to update entry in lookup table");

        let core = make_core(random_identifier(), Box::new(lt.clone()));
        let req = IdSearchReq::new(*core.id(), target, lvl, Direction::Left);
        let actual = core.search_by_id(&req).unwrap();

        let (expected_lvl, expected_identity) = lt
            .left_neighbors()
            .unwrap()
            .into_iter()
            .filter(|(l, id)| *l <= req.level() && id.id() >= req.target())
            .min_by_key(|(_, id)| *id.id())
            .unwrap();

        assert_eq!(expected_lvl, actual.termination_level());
        assert_eq!(*expected_identity.id(), *actual.result());
    }
}

/// Verifies right-direction search returns the greatest neighbor with identifier <= target.
#[test]
fn test_search_by_id_found_right_direction() {
    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
        let target = random_identifier();

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
        .expect("failed to update entry in lookup table");

        let core = make_core(random_identifier(), Box::new(lt.clone()));
        let req = IdSearchReq::new(*core.id(), target, lvl, Direction::Right);
        let actual = core.search_by_id(&req).unwrap();

        let (expected_lvl, expected_identity) = lt
            .right_neighbors()
            .unwrap()
            .into_iter()
            .filter(|(lvl, id)| *lvl <= req.level() && id.id() <= req.target())
            .max_by_key(|(_, id)| *id.id())
            .unwrap();

        assert_eq!(expected_lvl, actual.termination_level());
        assert_eq!(*expected_identity.id(), *actual.result());
    }
}

/// Verifies left-direction search falls back to the core's own identifier
/// when no neighbor satisfies the target.
#[test]
fn test_search_by_id_not_found_left_direction() {
    let target = random_identifier();

    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = ArrayLookupTable::new();
        for fill_lvl in 0..LOOKUP_TABLE_LEVELS {
            lt.update_entry(
                Identity::new(
                    &random_identifier_less_than(&target),
                    &random_membership_vector(),
                    random_address(),
                ),
                fill_lvl,
                Direction::Left,
            )
            .expect("failed to update entry in lookup table");
        }

        let core = make_core(random_identifier(), Box::new(lt.clone()));
        let req = IdSearchReq::new(*core.id(), target, lvl, Direction::Left);
        let actual = core.search_by_id(&req).unwrap();

        assert_eq!(actual.termination_level(), 0);
        assert_eq!(*actual.result(), *core.id());
    }
}

/// Verifies right-direction search falls back to the core's own identifier
/// when no neighbor satisfies the target.
#[test]
fn test_search_by_id_not_found_right_direction() {
    let target = random_identifier();

    for lvl in 0..LOOKUP_TABLE_LEVELS {
        let lt = ArrayLookupTable::new();
        for fill_lvl in 0..LOOKUP_TABLE_LEVELS {
            lt.update_entry(
                Identity::new(
                    &random_identifier_greater_than(&target),
                    &random_membership_vector(),
                    random_address(),
                ),
                fill_lvl,
                Direction::Right,
            )
            .expect("failed to update entry in lookup table");
        }

        let core = make_core(random_identifier(), Box::new(lt.clone()));
        let req = IdSearchReq::new(*core.id(), target, lvl, Direction::Right);
        let actual = core.search_by_id(&req).unwrap();

        assert_eq!(actual.termination_level(), 0);
        assert_eq!(*actual.result(), *core.id());
    }
}

/// Verifies `search_by_id` returns the exact match when the target exists in
/// the lookup table.
#[test]
fn test_search_by_id_exact_result() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let core = make_core(random_identifier(), Box::new(lt.clone()));

    for lvl in 0..LOOKUP_TABLE_LEVELS {
        for direction in [Direction::Left, Direction::Right] {
            let target_identity = lt.get_entry(lvl, direction).unwrap().unwrap();
            let target = target_identity.id();
            let req = IdSearchReq::new(*core.id(), *target, lvl, direction);
            let actual = core.search_by_id(&req).unwrap();

            assert_eq!(actual.termination_level(), lvl);
            assert_eq!(*actual.result(), *target);
        }
    }
}

/// Verifies left-direction `search_by_id` returns correct results under
/// concurrent access from 20 threads.
#[test]
fn test_search_by_id_concurrent_found_left_direction() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let target = random_identifier();
    let core: Box<dyn Core> = Box::new(make_core(random_identifier(), Box::new(lt.clone())));

    assert_ne!(&target, core.id());

    let num_threads = 20;
    let barrier = Arc::new(std::sync::Barrier::new(num_threads + 1));
    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for _ in 0..num_threads {
        let handle_barrier = barrier.clone();
        let core_ref = core.clone();
        let lt_clone = lt.clone();
        let handle = std::thread::spawn(move || {
            handle_barrier.wait();
            let lvl = rand::rng().random_range(0..LOOKUP_TABLE_LEVELS);
            let req = IdSearchReq::new(*core_ref.id(), target, lvl, Direction::Left);
            let actual = core_ref.search_by_id(&req).unwrap();

            let expected = lt_clone
                .left_neighbors()
                .unwrap()
                .into_iter()
                .filter(|(l, id)| *l <= req.level() && id.id() >= req.target())
                .min_by_key(|(_, id)| *id.id());

            match expected {
                Some((expected_lvl, expected_identity)) => {
                    assert_eq!(expected_lvl, actual.termination_level());
                    assert_eq!(*expected_identity.id(), *actual.result());
                }
                None => {
                    assert_eq!(actual.termination_level(), 0);
                    assert_eq!(*actual.result(), *core_ref.id());
                }
            }
        });
        handles.push(handle);
    }

    barrier.wait();
    let timeout = std::time::Duration::from_millis(1000);
    join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();
}

/// Verifies right-direction `search_by_id` returns correct results under
/// concurrent access from 20 threads.
#[test]
fn test_search_by_id_concurrent_right_direction() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let target = random_identifier();
    let core: Box<dyn Core> = Box::new(make_core(random_identifier(), Box::new(lt.clone())));

    assert_ne!(&target, core.id());

    let num_threads = 20;
    let barrier = Arc::new(std::sync::Barrier::new(num_threads + 1));
    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for _ in 0..num_threads {
        let handle_barrier = barrier.clone();
        let core_ref = core.clone();
        let lt_clone = lt.clone();
        let handle = std::thread::spawn(move || {
            handle_barrier.wait();
            let lvl = rand::rng().random_range(0..LOOKUP_TABLE_LEVELS);
            let req = IdSearchReq::new(*core_ref.id(), target, lvl, Direction::Right);
            let actual = core_ref.search_by_id(&req).unwrap();

            let expected = lt_clone
                .right_neighbors()
                .unwrap()
                .into_iter()
                .filter(|(l, id)| *l <= req.level() && id.id() <= req.target())
                .max_by_key(|(_, id)| *id.id());

            match expected {
                Some((expected_lvl, expected_identity)) => {
                    assert_eq!(expected_lvl, actual.termination_level());
                    assert_eq!(*expected_identity.id(), *actual.result());
                }
                None => {
                    assert_eq!(actual.termination_level(), 0);
                    assert_eq!(*actual.result(), *core_ref.id());
                }
            }
        });
        handles.push(handle);
    }
    barrier.wait();
    let timeout = std::time::Duration::from_millis(1000);
    join_all_with_timeout(handles.into_boxed_slice(), timeout).unwrap();
}

/// Verifies `search_by_id` propagates errors raised by the underlying lookup
/// table.
#[test]
fn test_search_by_id_error_propagation() {
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
            Err(anyhow!("simulated lookup table error"))
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

    let core = make_core(random_identifier(), Box::new(MockErrorLookupTable));
    let req = IdSearchReq::new(*core.id(), random_identifier(), 3, Direction::Left);
    let result = core.search_by_id(&req);

    assert!(
        result.is_err(),
        "expected an error but got a success result"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("error while searching by id in level"),
        "error message '{error_msg}' doesn't contain expected text"
    );
    assert!(
        error_msg.contains("simulated lookup table error"),
        "error message '{error_msg}' doesn't contain expected text"
    );
}
