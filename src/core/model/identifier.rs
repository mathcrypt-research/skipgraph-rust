use crate::core::model;
use crate::core::model::identifier::ComparisonResult::{CompareEqual, CompareGreater, CompareLess};
use crate::core::model::IDENTIFIER_SIZE_BYTES;
use anyhow::anyhow;
use std::fmt;
use std::fmt::{Debug, Display, Formatter};

/// A constant representing the zero value for the `Identifier` type.
///
/// This constant initializes an `Identifier` with all bytes set to zero.
pub const ZERO: Identifier = Identifier([0u8; IDENTIFIER_SIZE_BYTES]);

/// `MAX` is a constant of type `Identifier` that represents the maximum possible value
/// for an `Identifier`. It is initialized with an array of 255 (the maximum value for a
/// single byte) repeated across all elements of the array, with a size equal to
/// `IDENTIFIER_SIZE_BYTES`.
pub const MAX: Identifier = Identifier([255u8; IDENTIFIER_SIZE_BYTES]);

/// ComparisonResult represents the result of comparing two identifiers.
/// It can be one of the following:
/// - CompareGreater: the left identifier is greater than the right identifier.
/// - CompareEqual: the two identifiers are equal.
/// - CompareLess: the left identifier is less than the right identifier.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ComparisonResult {
    CompareGreater,
    CompareEqual,
    CompareLess,
}

/// ComparisonContext represents the context of a comparison between two identifiers.
/// It contains the result of the comparison, the left and right identifiers, and the index of the differing byte.
/// The differing byte is the first byte where the two identifiers differ.
pub struct ComparisonContext {
    result: ComparisonResult,
    left: Identifier,
    right: Identifier,
    diff_index: usize,
}

impl ComparisonContext {
    /// Returns the result of the comparison.
    pub fn result(&self) -> ComparisonResult {
        self.result
    }

    /// Returns the left identifier.
    pub fn left(&self) -> &Identifier {
        &self.left
    }

    /// Returns the right identifier.
    pub fn right(&self) -> &Identifier {
        &self.right
    }

    /// Returns the index of the differing byte.
    pub fn diff_index(&self) -> usize {
        self.diff_index
    }
}

/// Display overloads the Display trait for ComparisonContext, allowing it to be printed upon a call to format! or to_string().
impl Display for ComparisonContext {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.result {
            CompareGreater => write!(
                f,
                "{} > {} (at byte {})",
                &hex::encode(&self.left.0[0..=self.diff_index]),
                &hex::encode(&self.right.0[0..=self.diff_index]),
                self.diff_index
            ),
            CompareEqual => write!(f, "{} == {}", self.left, self.right),
            CompareLess => write!(
                f,
                "{} < {} (at byte {})",
                &hex::encode(&self.left.0[0..=self.diff_index]),
                &hex::encode(&self.right.0[0..=self.diff_index]),
                self.diff_index
            ),
        }
    }
}

// Identifier represents a 32-byte unique identifier for a Skip Graph node.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Identifier([u8; IDENTIFIER_SIZE_BYTES]);

impl Identifier {
    pub fn compare(&self, other: &Identifier) -> ComparisonContext {
        for i in 0..model::IDENTIFIER_SIZE_BYTES {
            match self.0[i].cmp(&other.0[i]) {
                std::cmp::Ordering::Less => {
                    return ComparisonContext {
                        result: CompareLess,
                        left: *self,
                        right: *other,
                        diff_index: i,
                    };
                }
                std::cmp::Ordering::Greater => {
                    return ComparisonContext {
                        result: CompareGreater,
                        left: *self,
                        right: *other,
                        diff_index: i,
                    };
                }
                _ => {}
            }
        }
        ComparisonContext {
            result: CompareEqual,
            left: *self,
            right: *other,
            diff_index: IDENTIFIER_SIZE_BYTES,
        }
    }

    /// Converts the input byte slice into an Identifier. The input must be at most 32 bytes long.
    /// If the input is less than 32 bytes, it will be padded with zeros from the left.
    /// If the input is more than 32 bytes, an error will be returned.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Identifier> {
        if bytes.len() > model::IDENTIFIER_SIZE_BYTES {
            return Err(anyhow!(
                "Identifier size is too large, expected {} bytes, got {} bytes",
                model::IDENTIFIER_SIZE_BYTES,
                bytes.len()
            ));
        }
        let mut identifier = [0; model::IDENTIFIER_SIZE_BYTES];
        let offset = model::IDENTIFIER_SIZE_BYTES - bytes.len();
        identifier[offset..].copy_from_slice(bytes);
        Ok(Identifier(identifier))
    }

    /// Converts the input hex string into an Identifier. The input must be at most 32 characters long.
    /// Note: the input string is expected to be a valid base58 string (NOT a hex string).
    pub fn from_string(s: &str) -> anyhow::Result<Identifier> {
        let decoded = hex::decode(s)?;
        Identifier::from_bytes(&decoded)
    }

    /// Converts the Identifier into a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Converts the Identifier into a owned byte vector.
    /// Consider using `as_bytes()` if you don't need ownership.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl Display for Identifier {
    /// Converts the Identifier into a base hex string.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

// Override Debug to also call Display
impl Debug for Identifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // This ensures both {:?} and {:#?} produce the same output as Display.
        write!(f, "{self}")
    }
}

impl Ord for Identifier {
    fn cmp(&self, other: &Identifier) -> std::cmp::Ordering {
        match self.compare(other).result {
            CompareLess => std::cmp::Ordering::Less,
            CompareEqual => std::cmp::Ordering::Equal,
            CompareGreater => std::cmp::Ordering::Greater,
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Identifier) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::random_identifier;
    use crate::core::testutil::random::random_hex_str;
    use crate::core::testutil::*;

    /// Tests the `Identifier::from_bytes` method with various types and sizes of byte arrays.
    ///
    /// This test covers the following scenarios:
    /// 1. Converts 32 bytes of zeros into an `Identifier` and asserts that converting back
    ///    to bytes gives the same input.
    /// 2. Converts 32 bytes of ones (all `255u8`) into an `Identifier` and asserts that
    ///    converting back to bytes gives the same input.
    /// 3. Converts 32 bytes of randomly generated data into an `Identifier` and asserts that
    ///    converting back to bytes gives the same input.
    /// 4. Converts 31 bytes of randomly generated data into an `Identifier` and checks that
    ///    the resulting `Identifier` is padded with a zero byte at the beginning when
    ///    converting back to bytes.
    /// 5. Attempts to convert 33 bytes of randomly generated data into an `Identifier`.
    ///    Asserts that this returns an error since the input length exceeds the valid size.
    ///
    /// Dependencies:
    /// - `Identifier::from_bytes` is expected to process exactly `IDENTIFIER_SIZE_BYTES` bytes
    ///   as input, pad with zero when the input is below the expected size, and return an
    ///   error when the input exceeds the expected size.
    /// - The `random::bytes` function is used to generate random input data for testing.
    ///
    /// Note:
    /// - `IDENTIFIER_SIZE_BYTES` is assumed to be 32 based on the context of the test.
    /// - The test cases verify both the ability to create `Identifier` from bytes and verify
    ///   round-trip consistency when converting the `Identifier` back to bytes.
    #[test]
    fn test_identifier_from_bytes() {
        // 32 bytes of zero
        let bytes = [0u8; IDENTIFIER_SIZE_BYTES];
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes(), bytes.to_vec());

        // 32 bytes of one
        let bytes = [255u8; IDENTIFIER_SIZE_BYTES];
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes(), bytes.to_vec());

        // 32 bytes random input
        let bytes = random::bytes(IDENTIFIER_SIZE_BYTES);
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes(), bytes);

        // 31 bytes random input; should be padded with 0
        let bytes = random::bytes(IDENTIFIER_SIZE_BYTES - 1);
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes()[1..], bytes);
        assert_eq!(identifier.to_bytes()[0], 0);

        // 33 bytes random input; should return an error
        let bytes = random::bytes(model::IDENTIFIER_SIZE_BYTES + 1);
        let result = Identifier::from_bytes(&bytes);
        assert!(result.is_err());
    }

    /// Unit tests for the `from_string` method of the `Identifier` struct.
    ///
    /// These tests validate the functionality and error handling behavior of the `from_string` method
    /// when working with input strings of varying lengths and content:
    ///
    /// - Correctly converts 32-byte zero-padded hexadecimal input into an `Identifier`.
    /// - Correctly converts 32-byte all-ones hexadecimal input into an `Identifier`.
    /// - Correctly converts random 32-byte hexadecimal input into an `Identifier`.
    /// - Handles 31-byte input by left-padding with zeroes to create a valid `Identifier`.
    /// - Returns an error when given input that exceeds 32 bytes.
    ///
    /// Test Scenarios:
    ///
    /// 1. **32 Bytes All Zeroes**
    ///    - Input: A hexadecimal string representing 32 bytes of zero (`0x00`).
    ///    - Behavior: Converts this into an `Identifier` and ensures its underlying byte array equals 32 bytes of zero.
    ///
    /// 2. **32 Bytes All Ones**
    ///    - Input: A hexadecimal string representing 32 bytes of `0xFF`.
    ///    - Behavior: Converts this into an `Identifier` and ensures its underlying byte array equals 32 bytes of `0xFF`.
    ///
    /// 3. **Random 32 Bytes**
    ///    - Input: A valid random 32-byte hexadecimal string.
    ///    - Behavior: Converts this into an `Identifier` and ensures its underlying byte representation matches the expected byte array (decoded from the input string).
    ///
    /// 4. **31 Bytes**
    ///    - Input: A valid random 31-byte hexadecimal string.
    ///    - Behavior: Converts this into an `Identifier` by left-padding one byte of zero, and ensures the resulting byte array starts with a zero byte followed by the expected bytes for the 31-byte input.
    ///
    /// 5. **33 Bytes**
    ///    - Input: A valid random 33-byte hexadecimal string.
    ///    - Behavior: Fails and returns an error, signaling that only input up to 32 bytes is allowed.
    #[test]
    fn test_identifier_from_string() {
        // 32 bytes zero
        let s = hex::encode(vec![0u8; model::IDENTIFIER_SIZE_BYTES]);
        let identifier = Identifier::from_string(&s).unwrap();
        assert_eq!(
            identifier.to_bytes(),
            vec![0u8; model::IDENTIFIER_SIZE_BYTES]
        );

        // 32 bytes one
        let s = hex::encode(vec![255u8; model::IDENTIFIER_SIZE_BYTES]);
        let identifier = Identifier::from_string(&s).unwrap();
        assert_eq!(
            identifier.to_bytes(),
            vec![255u8; model::IDENTIFIER_SIZE_BYTES]
        );

        // 32 bytes random input
        let s = random_hex_str(32);
        let identifier = Identifier::from_string(&s).unwrap();
        let expected_bytes = hex::decode(s).unwrap();
        assert_eq!(identifier.to_bytes(), expected_bytes);

        // 31 bytes should be left-padded from zero
        let s = random_hex_str(31);
        let identifier = Identifier::from_string(&s).unwrap();
        let expected_bytes = hex::decode(s).unwrap();
        assert_eq!(identifier.to_bytes()[1..], expected_bytes);
        assert_eq!(identifier.to_bytes()[0], 0);

        // 33 bytes should return an error
        let s = random_hex_str(33);
        assert!(Identifier::from_string(&s).is_err())
    }

    /// Test function `test_identifier_compare` verifies the behavior and correctness of the `Identifier`
    /// comparison functionality implemented in the system. It tests various comparison scenarios including:
    ///
    /// 1. Equality comparisons for identifiers that are identical.
    /// 2. Ordering comparisons (less than, greater than) between identifiers with different values.
    /// 3. Single-byte differences between two identifiers at a specific index.
    ///
    /// The test ensures that all aspects of the `compare` method are working as intended,
    /// including:
    /// - Determining the comparison result (`CompareEqual`, `CompareLess`, `CompareGreater`).
    /// - Validating that the left-hand side and right-hand side values are accurately assigned.
    /// - Identifying the first differing byte index when applicable.
    /// - Properly formatting the comparison result string.
    ///
    /// Specifically:
    /// - It creates multiple `Identifier` instances with predefined and random values.
    /// - Asserts that identifiers are equal to themselves.
    /// - Validates comparisons between identifiers, ensuring the results and metadata (e.g., differing byte index)
    ///   meet expectations.
    /// - Tests comparisons of identifiers generated with random byte sequences that differ only at
    ///   a specific location.
    ///
    /// Examples of tested comparisons:
    /// - Equality: Verifies that `id_0.compare(&id_0)` correctly determines equality (`id_0 == id_0`).
    /// - Ordering: Verifies relationships like `id_0 < id_1` and `id_1 > id_0`.
    /// - Single-byte difference: Confirms correctness for identifiers differing by a single byte, ensuring the differing
    ///   byte index is identified and result is consistent.
    ///
    /// This ensures the integrity and performance of the `Identifier` comparison logic across edge cases.
    #[test]
    fn test_identifier_compare() {
        let id_0 = Identifier::from_bytes(&[0u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
        let id_1 = Identifier::from_bytes(&[127u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
        let id_2 = Identifier::from_bytes(&[255u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();

        // each id is equal to itself
        let comp = id_0.compare(&id_0);
        assert_eq!(id_0, id_0);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_0, comp.left);
        assert_eq!(id_0, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(comp.to_string(), format!("{id_0} == {id_0}"));

        let comp = id_1.compare(&id_1);
        assert_eq!(id_1, id_1);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_1, comp.left);
        assert_eq!(id_1, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(comp.to_string(), format!("{id_1} == {id_1}"));

        let comp = id_2.compare(&id_2);
        assert_eq!(id_2, id_2);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_2, comp.left);
        assert_eq!(id_2, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(comp.to_string(), format!("{id_2} == {id_2}"));

        // id_0 < id_1
        let comp = id_0.compare(&id_1);
        assert!(id_0 < id_1);
        assert_eq!(CompareLess, comp.result);
        assert_eq!(id_0, comp.left);
        assert_eq!(id_1, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "00 < 7f (at byte 0)");

        let comp = id_1.compare(&id_0);
        assert!(id_1 > id_0);
        assert_eq!(CompareGreater, comp.result);
        assert_eq!(id_1, comp.left);
        assert_eq!(id_0, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "7f > 00 (at byte 0)");

        // id_1 < id_2
        let comp = id_1.compare(&id_2);
        assert!(id_1 < id_2);
        assert_eq!(CompareLess, comp.result);
        assert_eq!(id_1, comp.left);
        assert_eq!(id_2, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "7f < ff (at byte 0)");

        let comp = id_2.compare(&id_1);
        assert!(id_2 > id_1);
        assert_eq!(CompareGreater, comp.result);
        assert_eq!(id_2, comp.left);
        assert_eq!(id_1, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "ff > 7f (at byte 0)");

        // id_0 < id_2
        let comp = id_0.compare(&id_2);
        assert!(id_0 < id_2);
        assert_eq!(CompareLess, comp.result);
        assert_eq!(id_0, comp.left);
        assert_eq!(id_2, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "00 < ff (at byte 0)");

        let comp = id_2.compare(&id_0);
        assert!(id_2 > id_0);
        assert_eq!(CompareGreater, comp.result);
        assert_eq!(id_2, comp.left);
        assert_eq!(id_0, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "ff > 00 (at byte 0)");

        // two random identifiers composed that differ only in one byte
        // [left, 0, right] < [left, 1, right]
        let differing_byte_index = model::IDENTIFIER_SIZE_BYTES / 2;
        let left_bytes = random::bytes(differing_byte_index - 1);
        let right_bytes = random::bytes(differing_byte_index - 1);

        let random_greater = [left_bytes.clone(), vec![1u8; 1], right_bytes.clone()].concat();
        let id_random_greater = Identifier::from_bytes(&random_greater).unwrap();

        let random_less = [left_bytes, vec![0u8; 1], right_bytes].concat();
        let id_random_less = Identifier::from_bytes(&random_less).unwrap();

        // each identifier is equal to itself
        let comp = id_random_greater.compare(&id_random_greater);
        assert_eq!(id_random_greater, id_random_greater);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_random_greater, comp.left);
        assert_eq!(id_random_greater, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(
            comp.to_string(),
            format!("{id_random_greater} == {id_random_greater}")
        );

        let comp = id_random_less.compare(&id_random_less);
        assert_eq!(id_random_less, id_random_less);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_random_less, comp.left);
        assert_eq!(id_random_less, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);

        // id_random_greater > id_random_less
        let comp = id_random_greater.compare(&id_random_less);
        assert!(id_random_greater > id_random_less);
        assert_eq!(CompareGreater, comp.result);
        assert_eq!(id_random_greater, comp.left);
        assert_eq!(id_random_less, comp.right);
        assert_eq!(differing_byte_index, comp.diff_index);
        assert_eq!(
            comp.to_string(),
            format!(
                "{} > {} (at byte {})",
                &hex::encode(&id_random_greater.to_bytes()[0..=differing_byte_index]),
                &hex::encode(&id_random_less.to_bytes()[0..=differing_byte_index]),
                differing_byte_index
            )
        );

        // id_random_less < id_random_greater
        let comp = id_random_less.compare(&id_random_greater);
        assert!(id_random_less < id_random_greater);
        assert_eq!(CompareLess, comp.result);
        assert_eq!(id_random_less, comp.left);
        assert_eq!(id_random_greater, comp.right);
        assert_eq!(differing_byte_index, comp.diff_index);
        assert_eq!(
            comp.to_string(),
            format!(
                "{} < {} (at byte {})",
                &hex::encode(&id_random_less.to_bytes()[0..=differing_byte_index]),
                &hex::encode(&id_random_greater.to_bytes()[0..=differing_byte_index]),
                differing_byte_index
            )
        );
    }

    /// Tests the conversion of an `Identifier` to a `String` and back to an `Identifier`.
    ///
    /// This test generates a random `Identifier`, converts it to a `String` representation,
    /// and then attempts to convert it back into an `Identifier` using the `from_string` method.
    /// Finally, it asserts that the original `Identifier` and the resulting `Identifier`
    /// are equal, verifying the correctness and consistency of the conversion process.
    #[test]
    fn test_identifier_to_string() {
        let id = random_identifier();
        let id_str = id.to_string();
        let id_from_str = Identifier::from_string(&id_str).unwrap();
        assert_eq!(id, id_from_str);
    }
}
