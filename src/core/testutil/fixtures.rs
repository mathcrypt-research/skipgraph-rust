mod test_imports {
    pub use crate::core::model::direction::Direction;
    pub use crate::core::model::identity::Identity;
    pub use crate::core::testutil::random::random_hex_str;
    pub use crate::core::{
        model, Address, ArrayLookupTable, Identifier, LookupTable, LookupTableLevel,
        MembershipVector,
    };
    pub use rand::Rng;
}

use crate::core::model::identifier::{MAX, ZERO};
use std::thread::JoinHandle;
use std::time::Duration;
use test_imports::*;

pub fn random_identifier() -> Identifier {
    Identifier::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

/// Panics if `target` is `MAX`.
pub fn random_identifier_greater_than(target: &Identifier) -> Identifier {
    match *target {
        ZERO => random_identifier(),
        MAX => {
            panic!("cannot generate a random identifier greater than the maximum identifier.");
        }
        _ => {
            let mut bytes = target.to_bytes();
            for byte in bytes.iter_mut().rev() {
                if *byte < 0xFF {
                    *byte += 1;
                    break;
                }
            }
            Identifier::from_bytes(&bytes).unwrap_or_else(|_| {
                panic!("failed to create a valid identifier from bytes: {bytes:?}")
            })
        }
    }
}

/// Panics if `target` is `ZERO`.
pub fn random_identifier_less_than(target: &Identifier) -> Identifier {
    match *target {
        ZERO => {
            panic!("cannot generate a random identifier less than zero.");
        }
        MAX => random_identifier(),
        _ => {
            let mut bytes = target.to_bytes();
            for byte in bytes.iter_mut() {
                if *byte > 0x00 {
                    *byte -= 1;
                    break;
                }
            }

            Identifier::from_bytes(&bytes).unwrap_or_else(|_| {
                panic!("failed to create a valid identifier from bytes: {bytes:?}")
            })
        }
    }
}

pub fn random_sorted_identifiers(n: usize) -> Vec<Identifier> {
    let mut ids: Vec<Identifier> = (0..n).map(|_| random_identifier()).collect();
    ids.sort();
    ids
}

pub fn random_membership_vector() -> MembershipVector {
    MembershipVector::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

pub fn random_port() -> u16 {
    rand::rng().random_range(1024..=65535)
}

/// Uses `"localhost"` as the hostname.
pub fn random_address() -> Address {
    Address::new("localhost", &random_port().to_string())
}

pub fn random_identity() -> Identity {
    Identity::new(
        random_identifier(),
        random_membership_vector(),
        random_address(),
    )
}

pub fn random_identities(n: usize) -> Vec<Identity> {
    (0..n).map(|_| random_identity()).collect()
}

/// Inserts `2 * n` entries: at each level `i` in `0..n`, a `Left` and a `Right` neighbor.
pub fn random_lookup_table(n: usize) -> ArrayLookupTable {
    let lt = ArrayLookupTable::new();
    let ids = random_identities(2 * n);
    for i in 0..n {
        lt.update_entry(ids[i], i, Direction::Left).unwrap();
        lt.update_entry(ids[i + n], i, Direction::Right).unwrap();
    }
    lt
}

/// Sets the level-0 `Left` neighbor to the zero identifier/membership vector and the `Right`
/// neighbor to the maximum, so any left/right search has a neighbor on that direction.
pub fn random_lookup_table_with_extremes(n: usize) -> ArrayLookupTable {
    let lt = random_lookup_table(n);
    let zero_id = Identifier::from_bytes(&[0u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
    let zero_mv = MembershipVector::from_bytes(&[0u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();

    let max_id = Identifier::from_bytes(&[0xFFu8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
    let max_mv = MembershipVector::from_bytes(&[0xFFu8; model::IDENTIFIER_SIZE_BYTES]).unwrap();

    let zero_identity = Identity::new(zero_id, zero_mv, random_address());
    let max_identity = Identity::new(max_id, max_mv, random_address());

    lt.update_entry(zero_identity, 0, Direction::Left).unwrap();
    lt.update_entry(max_identity, 0, Direction::Right).unwrap();

    lt
}

/// The timeout is a global budget across all handles, joined sequentially. On timeout it returns
/// immediately, leaving the remaining handles unjoined.
pub fn join_all_with_timeout<T>(
    handles: Box<[JoinHandle<T>]>,
    timeout: Duration,
) -> Result<(), String>
where
    T: Send + 'static,
{
    let start = std::time::Instant::now();

    for handle in handles {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err("timeout".to_string());
        }

        let remaining_time = timeout - elapsed;

        match join_with_timeout(handle, remaining_time) {
            Ok(_) => continue,
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Joins on a helper thread so the join itself can be bounded by `timeout`.
pub fn join_with_timeout<T>(handle: JoinHandle<T>, timeout: Duration) -> Result<(), String>
where
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();

    let join_thread = std::thread::spawn(move || {
        let res = handle.join();
        let _ = tx.send(res);
    });

    if let Ok(join_res) = rx.recv_timeout(timeout) {
        join_thread.join().expect("failed to join thread");
        match join_res {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("thread panicked: {e:?}")),
        }
    } else {
        Err("thread timed out".to_string())
    }
}

/// Initializes the global tracing subscriber at DEBUG level (idempotent via `try_init`) and returns
/// a DEBUG-level span.
pub fn span_fixture() -> tracing::Span {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .try_init();

    tracing::span!(tracing::Level::DEBUG, "test_span")
}

/// Asserts that the nodes hold each other as immediate neighbors at `level`, forming one
/// doubly-linked list in identifier order. Every node's left slot must hold the node before it
/// and its right slot must hold the node after it. The two end nodes must hold `None` on their
/// outward slot.
///
/// A lookup table carries no record of its own owner's identifier, so `ids` supplies it.
/// `ids[i]` owns `tables[i]`, and `ids` must already be in ascending order. Panics if the two
/// slices have different lengths, since that would silently leave nodes unchecked.
///
/// # Args
/// * `ids`: the node identifiers, ascending.
/// * `tables`: each node's lookup table, in the same order as `ids`.
/// * `level`: the lookup-table level to check.
pub fn assert_doubly_linked_at_level<L: LookupTable>(
    ids: &[Identifier],
    tables: &[L],
    level: LookupTableLevel,
) {
    assert_eq!(
        ids.len(),
        tables.len(),
        "one lookup table per identifier is required"
    );

    for (i, lt) in tables.iter().enumerate() {
        let below = if i == 0 { None } else { Some(ids[i - 1]) };
        let above = ids.get(i + 1).copied();
        assert_eq!(
            lt.get_entry(level, Direction::Left)
                .expect("get_entry should not error")
                .map(|e| e.id()),
            below,
            "node {i}'s own left slot at level {level} must hold the node below it in the chain"
        );
        assert_eq!(
            lt.get_entry(level, Direction::Right)
                .expect("get_entry should not error")
                .map(|e| e.id()),
            above,
            "node {i}'s own right slot at level {level} must hold the node above it in the chain"
        );
    }
}

mod test {
    use crate::core::model::identifier::ComparisonResult::CompareLess;
    use crate::core::model::identifier::{MAX, ZERO};

    #[test]
    fn test_random_identifiers() {
        let ids = super::random_sorted_identifiers(100);

        ids.iter().skip(1).fold(&ids[0], |prev, curr| {
            assert_eq!(CompareLess, prev.compare(curr).result());
            curr
        });
    }

    #[test]
    fn test_random_identifier_greater_than() {
        let mut failure_count = 0;
        for _ in 0..1000 {
            let target = super::random_identifier();
            if target == MAX {
                failure_count += 1;
                continue;
            }
            let greater = super::random_identifier_greater_than(&target);

            assert!(greater > target);
        }
        assert!(
            failure_count < 1000,
            "failed to generate greater identifiers for all targets."
        );
    }

    #[test]
    fn test_random_identifier_less_than() {
        let mut failure_count = 0;
        for _ in 0..1000 {
            let target = super::random_identifier();
            if target == ZERO {
                failure_count += 1;
                continue;
            }
            let less = super::random_identifier_less_than(&target);

            assert!(less < target);
        }
        assert!(
            failure_count < 1000,
            "failed to generate lesser identifiers for all targets."
        );
    }
}

/// Polls `condition` on a blocking task (yielding between checks) until it is true or `timeout` elapses.
pub async fn wait_until<F>(mut condition: F, timeout: Duration) -> Result<(), String>
where
    F: FnMut() -> bool + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();

    let condition_task = tokio::task::spawn_blocking(move || loop {
        if condition() {
            let _ = tx.send(Ok(()));
            return;
        }
        std::thread::yield_now();
    });

    let result = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("channel closed unexpectedly".to_string()),
        Err(_) => Err(format!("condition not met within timeout of {:?}", timeout)),
    };

    condition_task.abort();

    result
}
