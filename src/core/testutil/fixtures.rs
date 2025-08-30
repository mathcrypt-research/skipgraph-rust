mod test_imports {
    pub use crate::core::model::direction::Direction;
    pub use crate::core::model::identity::Identity;
    pub use crate::core::testutil::random::random_hex_str;
    pub use crate::core::{
        model, Address, ArrayLookupTable, Identifier, LookupTable, MembershipVector,
    };
    pub use rand::Rng;
}

use crate::core::model::identifier::{MAX, ZERO};
use std::thread::JoinHandle;
use std::time::Duration;
use test_imports::*;

/// Generates a random identifier.
///
/// This function creates a random identifier by generating a random hex string
/// of `IDENTIFIER_SIZE_BYTES` length and converting it into an `Identifier`
/// type. The function unwraps the result of the `from_string` method, so it
/// assumes that the conversion will not fail.
///
/// # Returns
///
/// * `Identifier` - A randomly generated identifier.
///
/// # Panics
///
/// This function will panic if the `from_string` method returns an error, which
/// could happen if the generated random string does not comply with the expected
/// format of an `Identifier`.
pub fn random_identifier() -> Identifier {
    Identifier::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

/// Generates a random `Identifier` that is greater than the given target `Identifier`.
///
/// # Parameters
/// - `target`: A reference to an `Identifier` that acts as the lower bound (exclusive)
///   for the random identifier to be generated.
///
/// # Returns
/// An `Identifier` that is guaranteed to be greater than the provided `target`.
///
/// # Behavior
/// - If the `target` is equal to `ZERO`, the function generates a completely new random identifier.
/// - If the `target` is equal to `MAX` (the maximum possible identifier value), the function will
///   panic because it is not possible to create an identifier greater than `MAX`.
/// - For any other `target` value:
///   - The function modifies the bytes of the `target` such that the resulting bytes
///     represent a valid identifier greater than `target`.
///   - If needed, it attempts to resolve these modified bytes back into a valid `Identifier`.
///   - If resolving fails (unexpected), the function panics with an error message.
///
/// # Panics
/// - If the `target` is `MAX`, the function will panic with the message:
///   `"Cannot generate a random identifier greater than the maximum identifier."`
/// - If the modified bytes cannot be converted into a valid `Identifier`, the function will
///   panic and provide a debug description of the invalid bytes.
/// # Note
/// This function assumes that the `Identifier` type provides the following:
/// - A `to_bytes` method to convert the identifier into a mutable byte array.
/// - A `from_bytes` method to construct an identifier from byte data.
/// - Predefined constants like `ZERO` and `MAX` for boundary values.
pub fn random_identifier_greater_than(target: &Identifier) -> Identifier {
    match *target {
        ZERO => random_identifier(),
        MAX => {
            // If the target is the maximum identifier, we cannot generate a greater one.
            panic!("cannot generate a random identifier greater than the maximum identifier.");
        }
        _ => {
            // Keep making the bytes from the target identifier greater until we have a valid identifier.
            let mut bytes = target.to_bytes();
            for byte in bytes.iter_mut().rev() {
                if *byte < 0xFF {
                    *byte += 1; // Increment the byte to ensure it's greater
                    break;
                }
            }
            Identifier::from_bytes(&bytes).unwrap_or_else(|_| {
                panic!("failed to create a valid identifier from bytes: {bytes:?}")
            })
        }
    }
}

/// Generates a random `Identifier` that is less than a given `target` `Identifier`.
///
/// # Arguments
///
/// * `target` - A reference to an `Identifier` that serves as the upper bound.
///   The function will attempt to generate an `Identifier` less than this value.
///
/// # Returns
///
/// Returns a new `Identifier` that is guaranteed to be less than the provided `target`.
///
/// # Panics
///
/// * If the `target` is equal to `ZERO` (the minimum possible identifier),
///   the function will panic with the message:
///   `"Cannot generate a random identifier less than zero."`
///   since no value can be less than zero in this context.
///
/// * If the `Identifier::from_bytes` method fails during the creation of the new Identifier,
///   the function will panic with a message containing the invalid bytes being processed.
///
/// # Behavior
///
/// * If the `target` is `MAX` (the maximum possible identifier),
///   the function will generate and return a new random `Identifier` using the `random_identifier` method,
///   as any valid random identifier will satisfy the condition of being less than `MAX`.
///
/// * For any other valid `Identifier`, the function will attempt to decrement the bytes of the
///   `target` identifier (starting from the least-significant byte). It ensures
///   the resulting byte sequence is valid and uses it to construct the new `Identifier`.
///
/// # Notes
///
/// This function assumes that the `Identifier` type supports the following:
/// * A constant `ZERO` representing the smallest possible identifier.
/// * A constant `MAX` representing the largest possible identifier.
/// * A method `to_bytes` that converts the identifier into its byte representation.
/// * A method `from_bytes` that creates an identifier from a byte array, with error handling.
/// * The method `random_identifier` for generating a random valid identifier.
///
/// The exact structure of `Identifier`, as well as its byte representation,
/// is assumed to be consistent with this logic and behavior.
pub fn random_identifier_less_than(target: &Identifier) -> Identifier {
    match *target {
        ZERO => {
            // If the target is zero, we cannot generate a lesser identifier.
            panic!("cannot generate a random identifier less than zero.");
        }
        MAX => random_identifier(),
        _ => {
            // Keep making the bytes from the target identifier less until we have a valid identifier.
            let mut bytes = target.to_bytes();
            for byte in bytes.iter_mut() {
                if *byte > 0x00 {
                    *byte -= 1; // Decrement the byte to ensure it's less
                    break;
                }
            }

            Identifier::from_bytes(&bytes).unwrap_or_else(|_| {
                panic!("failed to create a valid identifier from bytes: {bytes:?}")
            })
        }
    }
}

/// Generates a vector of `n` randomly created and sorted `Identifier` values.
///
/// This function creates a collection of `Identifier` objects by calling
/// the `random_identifier` function `n` times. The resulting collection
/// is then sorted in ascending order before being returned.
///
/// # Arguments
/// * `n` - The number of random identifiers to generate.
///
/// # Returns
/// A `Vec<Identifier>` containing `n` randomly generated and sorted identifiers.
///
/// # Note
/// The `Identifier` type and the `random_identifier` function must be properly
/// defined in the scope where this function is used.
pub fn random_sorted_identifiers(n: usize) -> Vec<Identifier> {
    let mut ids: Vec<Identifier> = (0..n).map(|_| random_identifier()).collect();
    ids.sort();
    ids
}

/// Generates a random `MembershipVector`.
///
/// This function creates a `MembershipVector` using a randomly generated hexadecimal string
/// of a size determined by `model::IDENTIFIER_SIZE_BYTES`. The `random_hex_str` function is
/// used to generate the random hexadecimal string, which is then converted into a
/// `MembershipVector` using the `from_string` method. If the conversion fails, it will unwrap
/// and cause a panic.
///
/// # Returns
/// A randomly generated `MembershipVector`.
///
/// # Panics
/// This function will panic if the generated hexadecimal string cannot be converted into a
/// valid `MembershipVector`.
pub fn random_membership_vector() -> MembershipVector {
    MembershipVector::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

/// Generates a random port number within the range of valid ephemeral ports.
///
/// # Returns
///
/// A random `u16` value between 1024 and 65535 (inclusive).
pub fn random_port() -> u16 {
    rand::rng().random_range(1024..=65535)
}

/// Generates a random `Address`.
///
/// # Description
///
/// This function creates an `Address` with the hostname set to `"localhost"`
/// and a randomly generated port number.
///
/// # Returns
///
/// Returns an `Address` object populated with:
/// - Hostname: `"localhost"`
/// - Port: A randomly generated port number
///
/// # Dependencies
///
/// This function depends on:
/// - `random_port`: A utility function that generates a random port number.
/// - `Address::new`: A constructor method for creating a new `Address`
///   object by providing a hostname and a port.
pub fn random_address() -> Address {
    Address::new("localhost", &random_port().to_string())
}

/// Generates a random `Identity` object.
///
/// This function creates a new `Identity` instance by:
/// - Generating a random identifier using the `random_identifier` function.
/// - Generating a random membership vector using the `random_membership_vector` function.
/// - Generating a random address using the `random_address` function.
///
/// # Returns
///
/// A new `Identity` object populated with random values.
pub fn random_identity() -> Identity {
    Identity::new(
        &random_identifier(),
        &random_membership_vector(),
        random_address(),
    )
}

/// Generates a vector of random `Identity` objects.
///
/// This function creates `n` random identities by repeatedly calling the
/// `random_identity` function and collects them into a `Vec<Identity>`.
///
/// # Arguments
///
/// * `n` - The number of random identities to generate.
///
/// # Returns
///
/// A `Vec<Identity>` containing `n` randomly generated identities.
pub fn random_identities(n: usize) -> Vec<Identity> {
    (0..n).map(|_| random_identity()).collect()
}

/// Creates a random `ArrayLookupTable` with populated entries.
///
/// This function initializes a new `ArrayLookupTable` and populates its entries
/// with random identifiers and corresponding indices. For each index `i` in the
/// range `0..n`, two entries are added:
/// - One entry for the identifier at index `i`, using `Direction::Left`.
/// - One entry for the identifier at index `i + n`, using `Direction::Right`.
///
/// # Parameters
/// - `n`: The number of unique indices to be added to the lookup table. This will result
///   in a total of `2 * n` entries being inserted.
///
/// # Returns
/// - An `ArrayLookupTable` populated with randomly generated identifiers and their
///   associated indices.
///
/// # Panics
/// This function will panic if:
/// - The `update_entry` method on the `ArrayLookupTable` fails, which could happen
///   if the lookup table implementation imposes limits or constraints being exceeded.
///
/// # Note
/// - The `span_fixture` function is used to initialize the `ArrayLookupTable`.
/// - The `random_identities` function generates a list of random identifiers, whose length
///   must be at least `2 * n` for this function to work correctly.
///
/// # Dependencies
/// This code relies on the following external functions and structures:
/// - `ArrayLookupTable::new` to initialize the lookup table.
/// - `span_fixture()` to provide the necessary reference for lookup table creation.
/// - `random_identities` to generate a vector of random identifiers.
/// - `Direction` enum (likely defines `Left` and `Right` directions).
pub fn random_lookup_table(n: usize) -> ArrayLookupTable {
    let lt = ArrayLookupTable::new(&span_fixture());
    let ids = random_identities(2 * n);
    for i in 0..n {
        lt.update_entry(ids[i].clone(), i, Direction::Left).unwrap();
        lt.update_entry(ids[i + n].clone(), i, Direction::Right)
            .unwrap();
    }
    lt
}

/// Generates a random lookup table with extreme left and right entries at level 0.
/// The left most entry at level 0 is the zero identifier and membership vector,
/// and the right most entry is the maximum identifier and membership vector.
/// This is useful for testing edge cases in the lookup table, basically any search for an identifier
/// in left or right direction has a value among the neighbors on that direction.
pub fn random_lookup_table_with_extremes(n: usize) -> ArrayLookupTable {
    let lt = random_lookup_table(n);
    // Add extreme values to the lookup table
    let zero_id = Identifier::from_bytes(&[0u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
    let zero_mv = MembershipVector::from_bytes(&[0u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();

    let max_id = Identifier::from_bytes(&[0xFFu8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
    let max_mv = MembershipVector::from_bytes(&[0xFFu8; model::IDENTIFIER_SIZE_BYTES]).unwrap();

    let zero_identity = Identity::new(&zero_id, &zero_mv, random_address());
    let max_identity = Identity::new(&max_id, &max_mv, random_address());

    lt.update_entry(zero_identity, 0, Direction::Left).unwrap();
    lt.update_entry(max_identity, 0, Direction::Right).unwrap();

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
///    Arguments:
///    * handle: The JoinHandle<T> to join.
///    * timeout: The maximum time to wait for the thread to finish.
///   
///   Returns:
///
///   * Ok(()) if the thread finishes within the timeout.
///   * Err(String) if the thread takes longer than the timeout or panics.
pub fn join_with_timeout<T>(handle: JoinHandle<T>, timeout: Duration) -> Result<(), String>
where
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();

    // Spawn a thread just to join the target thread and send its result via channel
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

/// Create a tracing span fixture for testing purposes.
/// Note that this function initializes a global tracing subscriber for logging output,
/// at the DEBUG level (to avoid verbose output during tests and prolonged runtime, and hence
/// failures due to timeouts).
/// But the span itself is created at TRACE level.
pub fn span_fixture() -> tracing::Span {
    // Initialize the global tracing subscriber for logging output.
    // Using `try_init()` ensures that initialization happens only once globally,
    // avoiding panics on repeated calls (e.g., in multiple tests or repeated fixture usage).
    // Setting the max level to DEBUG enables debug-level logs to be captured and displayed.
    // This setup is necessary for `tracing::debug!` macros to produce visible output during tests or runtime.
    let _ = tracing_subscriber::fmt()
        // Change the subscriber to use a custom format if needed
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a new tracing span with the name "test_span" at TRACE level.
    // Subscriber level controls the minimum log level to display (e.g., DEBUG shows debug and above).
    // Log macros inside spans determine the actual log level of each event.
    // This span level is just a label for grouping and doesn’t influence what gets logged.
    tracing::span!(tracing::Level::TRACE, "test_span")
}

mod test {
    use crate::core::model::identifier::ComparisonResult::CompareLess;
    use crate::core::model::identifier::{MAX, ZERO};

    /// Tests the `random_sorted_identifiers` function.
    ///
    /// This test verifies that the `random_sorted_identifiers` function generates
    /// a collection of identifiers that are sorted in ascending order.
    ///
    /// Steps:
    /// 1. Calls the `random_sorted_identifiers` function with an argument specifying
    ///    that 100 identifiers should be generated.
    /// 2. Iterates through the generated identifiers to ensure that each identifier is
    ///    ordered correctly when compared to the previous one. The test asserts that the
    ///    result of comparing the current identifier with the previous one matches
    ///    `CompareLess`, meaning the identifiers are sorted in ascending order.
    ///
    /// If the identifiers are not sorted properly, the assertions within the test will fail.
    ///
    /// Note: The function `random_sorted_identifiers` and the `compare` method within
    /// the test are expected to be implemented in the `super` module context.
    /// ```
    #[test]
    fn test_random_identifiers() {
        let ids = super::random_sorted_identifiers(100);

        // ensures that the identifiers are sorted in ascending order
        ids.iter().skip(1).fold(&ids[0], |prev, curr| {
            assert_eq!(CompareLess, prev.compare(curr).result());
            curr
        });
    }

    /// Tests the `random_identifier_greater_than` function to ensure that it always generates
    /// an identifier greater than the given target identifier.
    ///
    /// The test generates 100 random target identifiers using the `random_identifier` function.
    /// For each target, it calls the `random_identifier_greater_than` function to generate
    /// an identifier that is supposed to be greater than the target.
    ///
    /// It then asserts that the generated identifier is indeed greater than the target
    /// using the `>` operator.
    ///
    /// This test verifies the correctness of the `random_identifier_greater_than` function.
    #[test]
    fn test_random_identifier_greater_than() {
        let mut failure_count = 0;
        for _ in 0..1000 {
            let target = super::random_identifier();
            if target == MAX {
                // If the target is the maximum identifier, we cannot generate a greater one.
                failure_count += 1;
                continue;
            }
            let greater = super::random_identifier_greater_than(&target);

            // Ensure that the generated identifier is indeed greater than the target
            assert!(greater > target);
        }
        assert!(
            failure_count < 1000,
            "failed to generate greater identifiers for all targets."
        );
    }

    /// Tests the `random_identifier_less_than` function from the parent module.
    ///
    /// The `random_identifier_less_than` function should generate a random identifier
    /// that is strictly less than the provided `target`. This test ensures the correctness
    /// of that behavior by running the function multiple times (1000 iterations) and
    /// verifying the conditions.
    ///
    /// Behavior:
    /// - If the generated `target` is equal to a defined constant `ZERO`,
    ///   it increments the `failure_count` and skips the assertion, as a lesser identifier
    ///   cannot be generated for `ZERO`.
    /// - Otherwise, it invokes `random_identifier_less_than` with the `target` and
    ///   asserts that the resulting value is strictly less than `target`.
    ///
    /// Additional Assertions:
    /// - The test ensures that not all iterations fail due to the target being `ZERO`
    ///   by asserting that `failure_count` remains less than 1000.
    ///
    /// Failure of this test would indicate:
    /// - `random_identifier_less_than` does not consistently return values less than `target`.
    /// - A significantly high number of generated `target` values equal to `ZERO`, indicating a
    ///   potential issue with the `random_identifier` function.
    #[test]
    fn test_random_identifier_less_than() {
        let mut failure_count = 0;
        for _ in 0..1000 {
            let target = super::random_identifier();
            if target == ZERO {
                // If the target is zero, we cannot generate a lesser one.
                failure_count += 1;
                continue;
            }
            let less = super::random_identifier_less_than(&target);

            // Ensure that the generated identifier is indeed less than the target
            assert!(less < target);
        }
        assert!(
            failure_count < 1000,
            "failed to generate lesser identifiers for all targets."
        );
    }
}
