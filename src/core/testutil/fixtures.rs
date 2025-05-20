
mod test_imports {
    pub use crate::core::model::direction::Direction;
    pub use crate::core::model::identity::Identity;
    pub use crate::core::testutil::random::random_hex_str;
    pub use crate::core::{model, Address, ArrayLookupTable, Identifier, LookupTable, MembershipVector};
    pub use rand::Rng;
}

use std::thread::JoinHandle;
use std::time::Duration;
use test_imports::*;


/// Generate a random identifier.
pub fn random_identifier() -> Identifier {
    Identifier::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

/// Generate n random identifiers sorted in ascending order.
pub fn random_sorted_identifiers(n: usize) -> Vec<Identifier> {
    let mut ids = (0..n)
        .map(|_| random_identifier())
        .collect::<Vec<Identifier>>();
    ids.sort();
    ids
}

/// Generate a random membership vector.
pub fn random_membership_vector() -> MembershipVector {
    MembershipVector::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

/// Generate a random port.
pub fn random_port() -> u16 {
    rand::rng().random_range(1024..=65535)
}

/// Generate a random address
pub fn random_address() -> Address {
    Address::new("localhost", &random_port().to_string())
}

/// Generate a random network identity; ID, MembershipVector, Address.
pub fn random_network_identity() -> Identity<Address> {
    Identity::new(
        &random_identifier(),
        &random_membership_vector(),
        random_address(),
    )
}

/// Generate n random network identities; ID, MembershipVector, Address.
pub fn random_network_identities(n: usize) -> Vec<Identity<Address>> {
    (0..n).map(|_| random_network_identity()).collect()
}

/// Generates a random lookup table with 2 * n entries (n left and n right), and n levels.
pub fn random_network_lookup_table(n: usize) -> ArrayLookupTable<Address> {
    let lt = ArrayLookupTable::new();
    let ids = random_network_identities(2 * n);
    for i in 0..n {
        lt.update_entry(ids[i], i, Direction::Left).unwrap();
        lt.update_entry(ids[i + n], i, Direction::Right).unwrap();
    }
    lt
}

/// Joins all threads in the given handles with a timeout.
///
/// This function attempts to join each thread sequentially, waiting up to the given timeout 
/// duration for each thread. If any thread does not complete within the remaining time budget, 
/// the function will immediately return an error.
///
/// Note: If a timeout occurs on any thread, the function does NOT attempt to join the remaining threads; 
/// it returns immediately. This means that some threads might remain unjoined if a timeout is encountered.
///
/// # Arguments
/// * `handles`: A boxed slice of `JoinHandle<T>` representing the threads to join.
/// * `timeout`: The maximum total time allowed to wait for all threads to finish.
///
/// # Returns
/// * `Ok(())` if all threads are joined successfully within the timeout.
/// * `Err(String)` if any thread exceeds the timeout or returns an error.
///
/// # Behavior
/// The timeout is applied globally across all threads, but checked sequentially based on elapsed time.
/// The function keeps track of elapsed time and reduces the wait time for each subsequent thread accordingly.
pub fn join_all_with_timeout<T>(handles : Box<[JoinHandle<T>]>, timeout: Duration) -> Result<(), String>
where T : Send + 'static {
    let start = std::time::Instant::now();

    for handle in handles {
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err("Timeout".to_string());
        }

        // Remaining time to wait for this thread to finish
        let remaining_time = timeout - elapsed;

        // Check if the thread has finished
        match join_with_timeout(handle, remaining_time) {
            Ok(_) => continue,
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Helper function to join a thread with a timeout using a simple trick:
/// 1. Spawn a new thread that will join the target thread.
/// 2. Use a channel to send the result of the join back to the main thread.
/// 3. If the join takes too long, the main thread will timeout and return an error.
/// Arguments:
/// * handle: The JoinHandle<T> to join.
/// * timeout: The maximum time to wait for the thread to finish.
/// Returns:
/// * Ok(()) if the thread finishes within the timeout.
/// * Err(String) if the thread takes longer than the timeout or panics.
pub fn join_with_timeout<T>(handle: JoinHandle<T>, timeout: Duration) -> Result<(), String>
where T : Send + 'static {
    let (tx, rx) = std::sync::mpsc::channel();

    // Spawn a thread just to join the target thread and send its result via channel
    let join_thread = std::thread::spawn(move || {
        let res = handle.join();
        let _ = tx.send(res);
    });

    if let Ok(join_res) = rx.recv_timeout(timeout) {
        join_thread.join().expect("Failed to join thread");
        match join_res {
            Ok(res) => Ok(()),
            Err(e) => Err(format!("Thread panicked: {:?}", e)),
        }
    } else {
        Err("Thread timed out".to_string())
    }
}

mod test {
    use crate::core::model::identifier::ComparisonResult::CompareLess;

    /// Test random identifier generation, generates 100 random identifiers and checks if they are sorted in ascending order.
    #[test]
    fn test_random_identifiers() {
        let ids = super::random_sorted_identifiers(100);

        // ensures that the identifiers are sorted in ascending order
        ids.iter().skip(1).fold(&ids[0], |prev, curr| {
            assert_eq!(CompareLess, prev.compare(curr).result());
            curr
        });
    }
}
