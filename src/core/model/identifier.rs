use crate::core::model;
use crate::core::model::identifier::ComparisonResult::{CompareEqual, CompareGreater, CompareLess};
use crate::core::model::IDENTIFIER_SIZE_BYTES;
use anyhow::{anyhow, Context};
use std::fmt;
use std::fmt::{Display, Formatter};

/// ComparisonResult represents the result of comparing two identifiers.
/// It can be one of the following:
/// - CompareGreater: the left identifier is greater than the right identifier.
/// - CompareEqual: the two identifiers are equal.
/// - CompareLess: the left identifier is less than the right identifier.
#[derive(Debug, PartialEq, Clone)]
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
        self.result.clone()
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub struct Identifier([u8; model::IDENTIFIER_SIZE_BYTES]);

impl Identifier {
    pub fn compare(&self, other: &Identifier) -> ComparisonContext {
        for i in 0..model::IDENTIFIER_SIZE_BYTES {
            if self.0[i] < other.0[i] {
                return ComparisonContext {
                    result: CompareLess,
                    left: self.clone(),
                    right: other.clone(),
                    diff_index: i,
                };
            } else if self.0[i] > other.0[i] {
                return ComparisonContext {
                    result: CompareGreater,
                    left: self.clone(),
                    right: other.clone(),
                    diff_index: i,
                };
            }
        }
        ComparisonContext {
            result: CompareEqual,
            left: self.clone(),
            right: other.clone(),
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
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl Display for Identifier {
    /// Converts the Identifier into a base hex string.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::random_membership_vector;
    use crate::core::testutil::random::random_hex_str;
    use crate::core::testutil::*;
    use std::io::Read;

    #[test]
    fn test_identifier_from_bytes() {
        // 32 bytes of zero
        let bytes = [0u8; model::IDENTIFIER_SIZE_BYTES];
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes(), bytes.to_vec());

        // 32 bytes of one
        let bytes = [255u8; model::IDENTIFIER_SIZE_BYTES];
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes(), bytes.to_vec());

        // 32 bytes random input
        let bytes = random::bytes(model::IDENTIFIER_SIZE_BYTES);
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes(), bytes);

        // 31 bytes random input; should be padded with 0
        let bytes = random::bytes(model::IDENTIFIER_SIZE_BYTES - 1);
        let identifier = Identifier::from_bytes(&bytes).unwrap();
        assert_eq!(identifier.to_bytes()[1..], bytes);
        assert_eq!(identifier.to_bytes()[0], 0);

        // 33 bytes random input; should return an error
        let bytes = random::bytes(model::IDENTIFIER_SIZE_BYTES + 1);
        let result = Identifier::from_bytes(&bytes);
        assert!(result.is_err());
    }

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

    #[test]
    fn test_identifier_compare() {
        let id_0 = Identifier::from_bytes(&vec![0u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
        let id_1 = Identifier::from_bytes(&vec![127u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();
        let id_2 = Identifier::from_bytes(&vec![255u8; model::IDENTIFIER_SIZE_BYTES]).unwrap();

        // each id is equal to itself
        let comp = id_0.compare(&id_0);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_0, comp.left);
        assert_eq!(id_0, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(comp.to_string(), format!("{} == {}", id_0, id_0));

        let comp = id_1.compare(&id_1);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_1, comp.left);
        assert_eq!(id_1, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(comp.to_string(), format!("{} == {}", id_1, id_1));

        let comp = id_2.compare(&id_2);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_2, comp.left);
        assert_eq!(id_2, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(comp.to_string(), format!("{} == {}", id_2, id_2));

        // id_0 < id_1
        let comp = id_0.compare(&id_1);
        assert_eq!(CompareLess, comp.result);
        assert_eq!(id_0, comp.left);
        assert_eq!(id_1, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "00 < 7f (at byte 0)");

        let comp = id_1.compare(&id_0);
        assert_eq!(CompareGreater, comp.result);
        assert_eq!(id_1, comp.left);
        assert_eq!(id_0, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "7f > 00 (at byte 0)");

        // id_1 < id_2
        let comp = id_1.compare(&id_2);
        assert_eq!(CompareLess, comp.result);
        assert_eq!(id_1, comp.left);
        assert_eq!(id_2, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "7f < ff (at byte 0)");

        let comp = id_2.compare(&id_1);
        assert_eq!(CompareGreater, comp.result);
        assert_eq!(id_2, comp.left);
        assert_eq!(id_1, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "ff > 7f (at byte 0)");

        // id_0 < id_2
        let comp = id_0.compare(&id_2);
        assert_eq!(CompareLess, comp.result);
        assert_eq!(id_0, comp.left);
        assert_eq!(id_2, comp.right);
        assert_eq!(0, comp.diff_index);
        assert_eq!(comp.to_string(), "00 < ff (at byte 0)");

        let comp = id_2.compare(&id_0);
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
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_random_greater, comp.left);
        assert_eq!(id_random_greater, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);
        assert_eq!(
            comp.to_string(),
            format!("{} == {}", id_random_greater, id_random_greater)
        );

        let comp = id_random_less.compare(&id_random_less);
        assert_eq!(CompareEqual, comp.result);
        assert_eq!(id_random_less, comp.left);
        assert_eq!(id_random_less, comp.right);
        assert_eq!(IDENTIFIER_SIZE_BYTES, comp.diff_index);

        // id_random_greater > id_random_less
        let comp = id_random_greater.compare(&id_random_less);
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

    #[test]
    fn test_identifier_to_string() {
        let id = random_membership_vector();
        let id_str = id.to_string();
        println!("{}", id_str);
    }
}
