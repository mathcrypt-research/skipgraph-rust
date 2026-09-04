use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use crate::core::model::search::Nonce;
use crate::core::testutil::fixtures::{
    join_all_with_timeout, random_address, random_identifier, random_identifier_greater_than,
    random_identifier_less_than, random_identity, random_lookup_table_with_extremes,
    random_membership_vector, span_fixture,
};
use crate::core::{
    ArrayLookupTable, IdSearchReq, Identifier, LinkOutcome, LookupTable, LookupTableMock,
    MembershipVector, RelinkOutcome, LOOKUP_TABLE_LEVELS,
};
use crate::node::core::{BaseCore, Core};
use anyhow::anyhow;
use rand::Rng;
use std::sync::Arc;
use unimock::*;

/// Verifies `search_by_id` returns the core's own identifier when the lookup
/// table is empty.
#[test]
fn test_search_by_id_singleton_fallback() {
    let origin_id = Identifier::from_bytes(&[10u8]).unwrap();
    let core = BaseCore::new(
        span_fixture(),
        origin_id,
        random_membership_vector(),
        Box::new(ArrayLookupTable::new()),
    );

    let cases = [
        (Identifier::from_bytes(&[5u8]).unwrap(), Direction::Left),
        (Identifier::from_bytes(&[15u8]).unwrap(), Direction::Left),
        (Identifier::from_bytes(&[5u8]).unwrap(), Direction::Right),
        (Identifier::from_bytes(&[15u8]).unwrap(), Direction::Right),
    ];

    for (target, direction) in cases {
        let req = IdSearchReq {
            nonce: Nonce::random(),
            origin: origin_id,
            target,
            level: 3,
            direction,
        };
        let res = core.search_by_id(req).expect("search failed");
        assert_eq!(res.termination_level, 0);
        assert_eq!(res.result, origin_id);
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
            Identity::new(safe_neighbor, random_membership_vector(), random_address()),
            0,
            Direction::Left,
        )
        .expect("failed to update entry in lookup table");

        let core = BaseCore::new(
            span_fixture(),
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
        );
        let req = IdSearchReq {
            nonce: Nonce::random(),
            origin: core.id(),
            target,
            level: lvl,
            direction: Direction::Left,
        };
        let actual = core.search_by_id(req).unwrap();

        let (expected_lvl, expected_identity) = lt
            .left_neighbors()
            .into_iter()
            .filter(|(l, id)| *l <= req.level && id.id() >= req.target)
            .min_by_key(|(_, id)| id.id())
            .unwrap();

        assert_eq!(expected_lvl, actual.termination_level);
        assert_eq!(expected_identity.id(), actual.result);
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
            Identity::new(safe_neighbor, random_membership_vector(), random_address()),
            0,
            Direction::Right,
        )
        .expect("failed to update entry in lookup table");

        let core = BaseCore::new(
            span_fixture(),
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
        );
        let req = IdSearchReq {
            nonce: Nonce::random(),
            origin: core.id(),
            target,
            level: lvl,
            direction: Direction::Right,
        };
        let actual = core.search_by_id(req).unwrap();

        let (expected_lvl, expected_identity) = lt
            .right_neighbors()
            .into_iter()
            .filter(|(lvl, id)| *lvl <= req.level && id.id() <= req.target)
            .max_by_key(|(_, id)| id.id())
            .unwrap();

        assert_eq!(expected_lvl, actual.termination_level);
        assert_eq!(expected_identity.id(), actual.result);
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
                    random_identifier_less_than(&target),
                    random_membership_vector(),
                    random_address(),
                ),
                fill_lvl,
                Direction::Left,
            )
            .expect("failed to update entry in lookup table");
        }

        let core = BaseCore::new(
            span_fixture(),
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
        );
        let req = IdSearchReq {
            nonce: Nonce::random(),
            origin: core.id(),
            target,
            level: lvl,
            direction: Direction::Left,
        };
        let actual = core.search_by_id(req).unwrap();

        assert_eq!(actual.termination_level, 0);
        assert_eq!(actual.result, core.id());
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
                    random_identifier_greater_than(&target),
                    random_membership_vector(),
                    random_address(),
                ),
                fill_lvl,
                Direction::Right,
            )
            .expect("failed to update entry in lookup table");
        }

        let core = BaseCore::new(
            span_fixture(),
            random_identifier(),
            random_membership_vector(),
            Box::new(lt.clone()),
        );
        let req = IdSearchReq {
            nonce: Nonce::random(),
            origin: core.id(),
            target,
            level: lvl,
            direction: Direction::Right,
        };
        let actual = core.search_by_id(req).unwrap();

        assert_eq!(actual.termination_level, 0);
        assert_eq!(actual.result, core.id());
    }
}

/// Verifies `search_by_id` returns the exact match when the target exists in
/// the lookup table.
#[test]
fn test_search_by_id_exact_result() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
    );

    for lvl in 0..LOOKUP_TABLE_LEVELS {
        for direction in [Direction::Left, Direction::Right] {
            let target_identity = lt.get_entry(lvl, direction).unwrap().unwrap();
            let target = target_identity.id();
            let req = IdSearchReq {
                nonce: Nonce::random(),
                origin: core.id(),
                target,
                level: lvl,
                direction,
            };
            let actual = core.search_by_id(req).unwrap();

            assert_eq!(actual.termination_level, lvl);
            assert_eq!(actual.result, target);
        }
    }
}

/// Verifies left-direction `search_by_id` returns correct results under
/// concurrent access from 20 threads.
#[test]
fn test_search_by_id_concurrent_found_left_direction() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let target = random_identifier();
    let core: Box<dyn Core> = Box::new(BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
    ));

    assert_ne!(target, core.id());

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
            let req = IdSearchReq {
                nonce: Nonce::random(),
                origin: core_ref.id(),
                target,
                level: lvl,
                direction: Direction::Left,
            };
            let actual = core_ref.search_by_id(req).unwrap();

            let expected = lt_clone
                .left_neighbors()
                .into_iter()
                .filter(|(l, id)| *l <= req.level && id.id() >= req.target)
                .min_by_key(|(_, id)| id.id());

            match expected {
                Some((expected_lvl, expected_identity)) => {
                    assert_eq!(expected_lvl, actual.termination_level);
                    assert_eq!(expected_identity.id(), actual.result);
                }
                None => {
                    assert_eq!(actual.termination_level, 0);
                    assert_eq!(actual.result, core_ref.id());
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
    let core: Box<dyn Core> = Box::new(BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
    ));

    assert_ne!(target, core.id());

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
            let req = IdSearchReq {
                nonce: Nonce::random(),
                origin: core_ref.id(),
                target,
                level: lvl,
                direction: Direction::Right,
            };
            let actual = core_ref.search_by_id(req).unwrap();

            let expected = lt_clone
                .right_neighbors()
                .into_iter()
                .filter(|(l, id)| *l <= req.level && id.id() <= req.target)
                .max_by_key(|(_, id)| id.id());

            match expected {
                Some((expected_lvl, expected_identity)) => {
                    assert_eq!(expected_lvl, actual.termination_level);
                    assert_eq!(expected_identity.id(), actual.result);
                }
                None => {
                    assert_eq!(actual.termination_level, 0);
                    assert_eq!(actual.result, core_ref.id());
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
    let lt = Unimock::new(
        LookupTableMock::get_entry
            .each_call(matching!(_, _))
            .answers(&|_, _, _| Err(anyhow!("simulated lookup table error"))),
    );

    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt),
    );
    let req = IdSearchReq {
        nonce: Nonce::random(),
        origin: core.id(),
        target: random_identifier(),
        level: 3,
        direction: Direction::Left,
    };
    let result = core.search_by_id(req);

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

/// Verifies `max_level` returns 0 when the lookup table has no populated entries.
#[test]
fn test_max_level_empty_table() {
    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(ArrayLookupTable::new()),
    );

    assert_eq!(core.max_level().unwrap(), 0);
}

/// Verifies `max_level` returns the highest populated level when only one side
/// of the table has entries.
#[test]
fn test_max_level_one_side_populated() {
    let lt = ArrayLookupTable::new();
    lt.update_entry(random_identity(), 2, Direction::Left)
        .expect("failed to update entry in lookup table");
    lt.update_entry(random_identity(), 5, Direction::Left)
        .expect("failed to update entry in lookup table");

    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt),
    );

    assert_eq!(core.max_level().unwrap(), 5);
}

/// Verifies `max_level` returns the highest populated level across both sides
/// when left and right are populated at different levels.
#[test]
fn test_max_level_both_sides_populated_different_levels() {
    let lt = ArrayLookupTable::new();
    lt.update_entry(random_identity(), 3, Direction::Left)
        .expect("failed to update entry in lookup table");
    lt.update_entry(random_identity(), 7, Direction::Right)
        .expect("failed to update entry in lookup table");

    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt),
    );

    assert_eq!(core.max_level().unwrap(), 7);
}

/// Verifies `prefix_match` matches `common_prefix_bit(candidate) >= level`
/// exactly, including at the boundary where they're equal.
#[test]
fn test_prefix_match() {
    // all-zero and all-one membership vectors share zero common prefix bits.
    let mv_zero = MembershipVector::from_bytes(&[0u8; 32]).unwrap();
    let mv_ones = MembershipVector::from_bytes(&[0xffu8; 32]).unwrap();
    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        mv_zero,
        Box::new(ArrayLookupTable::new()),
    );

    let common = mv_zero.common_prefix_bit(mv_ones);
    assert_eq!(common, 0);

    // true case: required level is below the actual common-prefix length.
    assert!(core.prefix_match(mv_zero, 0));
    // boundary case: required level equals the actual common-prefix length exactly.
    assert!(core.prefix_match(mv_ones, common));
    // false case: required level exceeds the actual common-prefix length.
    assert!(!core.prefix_match(mv_ones, common + 1));
}

/// Verifies `Core::try_link` delegates to the lookup table: an empty slot is
/// linked directly and the write is visible through the table.
#[test]
fn test_try_link_empty_slot() {
    let lt = ArrayLookupTable::new();
    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
    );
    let candidate = random_identity();

    let outcome = core
        .try_link(0, Direction::Left, candidate)
        .expect("try_link failed");

    assert_eq!(outcome, LinkOutcome::LinkedDirectly);
    assert_eq!(lt.get_entry(0, Direction::Left).unwrap(), Some(candidate));
}

/// Verifies `Core::try_relink` delegates to the lookup table: a slot already
/// holding the claimant is reported as consistent and left untouched.
#[test]
fn test_try_relink_already_consistent() {
    let lt = ArrayLookupTable::new();
    let claimant = random_identity();
    lt.update_entry(claimant, 0, Direction::Right)
        .expect("failed to update entry in lookup table");
    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(lt.clone()),
    );

    let outcome = core
        .try_relink(0, Direction::Right, claimant)
        .expect("try_relink failed");

    assert_eq!(outcome, RelinkOutcome::AlreadyConsistent);
    assert_eq!(lt.get_entry(0, Direction::Right).unwrap(), Some(claimant));
}

/// Verifies `Core::try_link` propagates the lookup table's out-of-range-level error.
#[test]
fn test_try_link_out_of_range_level() {
    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(ArrayLookupTable::new()),
    );

    let result = core.try_link(LOOKUP_TABLE_LEVELS, Direction::Left, random_identity());

    assert!(
        result.is_err(),
        "expected an error but got a success result"
    );
}

/// Verifies `Core::try_relink` propagates the lookup table's out-of-range-level error.
#[test]
fn test_try_relink_out_of_range_level() {
    let core = BaseCore::new(
        span_fixture(),
        random_identifier(),
        random_membership_vector(),
        Box::new(ArrayLookupTable::new()),
    );

    let result = core.try_relink(LOOKUP_TABLE_LEVELS, Direction::Left, random_identity());

    assert!(
        result.is_err(),
        "expected an error but got a success result"
    );
}
