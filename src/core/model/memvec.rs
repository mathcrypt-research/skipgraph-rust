use crate::core::model;
use anyhow::{anyhow, Context};
use std::fmt;
use std::fmt::{Debug, Display, Formatter};

#[derive(Copy, PartialEq, Eq)]
pub struct MembershipVector([u8; model::IDENTIFIER_SIZE_BYTES]);

#[allow(useless_deprecated)]
impl Clone for MembershipVector {
    #[deprecated(note = "This type is Copy; prefer implicit copying instead of .clone()")]
    fn clone(&self) -> Self {
        *self
    }
}

/// A struct representing a membership vector with a fixed size of 32 bytes.
impl MembershipVector {
    /// Formats the MembershipVector as a hexadecimal string.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A byte slice to be converted.
    ///
    /// # Returns
    ///
    /// * `anyhow::Result<MembershipVector>` - The resulting MembershipVector or an error if the input is too large.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<MembershipVector> {
        if bytes.len() > model::IDENTIFIER_SIZE_BYTES {
            Err(anyhow!(
                "membership vector size is too large, expected {} bytes, got {} bytes",
                model::IDENTIFIER_SIZE_BYTES,
                bytes.len()
            ))
        } else {
            let mut mv = [0u8; model::IDENTIFIER_SIZE_BYTES];
            let offset = model::IDENTIFIER_SIZE_BYTES - bytes.len();
            mv[offset..].copy_from_slice(bytes);
            Ok(MembershipVector(mv))
        }
    }

    /// Converts the input hex string into a MembershipVector. The input must be at most 32 hex characters long.
    ///
    /// # Arguments
    ///
    /// * `s` - A hex string to be converted.
    ///
    /// # Returns
    ///
    /// * `anyhow::Result<MembershipVector>` - The resulting MembershipVector or an error if the input is invalid.
    pub fn from_string(s: &str) -> anyhow::Result<MembershipVector> {
        let bytes = hex::decode(s).context("failed to decode hex string")?;
        MembershipVector::from_bytes(&bytes)
    }

    /// Converts the input string into a bit string.
    pub fn to_bit_string(&self) -> String {
        use std::fmt::Write;
        let mut result = String::with_capacity(self.0.len() * 9); // 8 bits + 1 space per byte
        for (i, &b) in self.0.iter().enumerate() {
            if i > 0 {
                result.push(' ');
            }
            write!(result, "{b:08b}").expect("writing to String should never fail");
        }
        result
    }

    /// Returns a reference to the underlying byte array.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Converts the MembershipVector into a byte slice.
    ///
    /// # Returns
    ///
    /// * `Vec<u8>` - A vector containing the bytes of the MembershipVector.
    /// 
    /// Consider using `as_bytes()` if you don't need ownership.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Calculates the number of common prefix bits between this MembershipVector and another.
    ///
    /// # Arguments
    ///
    /// * `other` - Another MembershipVector to compare with.
    ///
    /// # Returns
    ///
    /// * `u64` - The number of common prefix bits.
    pub fn common_prefix_bit(&self, other: &MembershipVector) -> usize {
        let mut common_bits = 0;
        for (byte_a, byte_b) in self.0.iter().zip(other.0.iter()) {
            let xor = byte_a ^ byte_b;
            if xor == 0 {
                // entire byte is a common prefix
                common_bits += 8;
                // move to next byte
                continue;
            }

            // move along XOR from MSB to LSB; count zero bits; break on the first non-zero
            for i in (0..8).rev() {
                // mask the ith bit of XOR
                if xor & (1 << i) != 0 {
                    // ith bit of XOR is non-zero; the first discrepancy; break
                    return common_bits;
                }
                // ith bit of XOR is zero; increment common bit
                common_bits += 1;
            }
        }

        common_bits
    }

    /// Decompose the prefix at a given pivot bit index.
    /// Returns a tuple of three strings:
    /// 1. The left part of the prefix in hex format.
    /// 2. The byte containing pivot bit in binary format.
    /// 3. The right part of the prefix in hex format.
    /// # Example:
    /// ```
    /// let mv = crate::skipgraph::core::MembershipVector::from_string("a738f14dc0750b7f7e4c418fdda32c424e9ecf7280892d14648e405466a76f29").unwrap();
    /// let (left, pivot, right) = mv.decompose_at_bit(40);
    /// assert_eq!(left, "a738f14dc0");
    /// assert_eq!(pivot, "01110101");
    /// assert_eq!(right, "0b7f7e4c418fdda32c424e9ecf7280892d14648e405466a76f29");
    /// ```
    pub fn decompose_at_bit(&self, bit_index: usize) -> (String, String, String) {
        let prefix_byte_index = bit_index / 8;
        let left_prefix = &self.0[..prefix_byte_index];

        if prefix_byte_index == model::IDENTIFIER_SIZE_BYTES - 1 {
            return (
                hex::encode(left_prefix),
                format!("{:08b}", self.0[prefix_byte_index]),
                String::new(),
            );
        }

        (
            hex::encode(left_prefix),
            format!("{:08b}", self.0[prefix_byte_index]),
            hex::encode(&self.0[prefix_byte_index + 1..]),
        )
    }
}

impl Display for MembershipVector {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

// Override Debug to also call Display
impl Debug for MembershipVector {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // This ensures both {:?} and {:#?} produce the same output as Display.
        write!(f, "{self}")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::core::testutil::fixtures::random_membership_vector;
    use crate::core::testutil::random;
    use rand::Rng;

    extern crate rand;
    use crate::core::testutil::random::random_hex_str;

    #[test]
    fn test_membership_vector_from_bytes() {
        use super::*;
        // 32 bytes of zero
        let bytes = [0u8; 32];
        let mv = MembershipVector::from_bytes(&bytes).unwrap();
        assert_eq!(mv.to_bytes(), bytes.to_vec());

        // 32 bytes of one
        let bytes = [255u8; 32];
        let mv = MembershipVector::from_bytes(&bytes).unwrap();
        assert_eq!(mv.to_bytes(), bytes.to_vec());

        // 32 bytes of random input
        let bytes = random::bytes(32);
        let mv = MembershipVector::from_bytes(&bytes).unwrap();
        assert_eq!(mv.to_bytes(), bytes);

        // 31 bytes random input; should be padded with 0
        let bytes = random::bytes(31);
        let mv = MembershipVector::from_bytes(&bytes).unwrap();
        assert_eq!(mv.to_bytes()[1..], bytes);
        assert_eq!(mv.to_bytes()[0], 0);

        // 33 bytes random input; should return an error
        let bytes = random::bytes(33);
        let result = MembershipVector::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_membership_vector_from_string() {
        // 32 bytes of zeros
        let s = hex::encode(vec![0; model::IDENTIFIER_SIZE_BYTES]);
        let mv = MembershipVector::from_string(&s).unwrap();
        assert_eq!(mv.to_bytes(), vec![0; model::IDENTIFIER_SIZE_BYTES]);

        // 32 bytes of ones
        let s = hex::encode(vec![255u8; model::IDENTIFIER_SIZE_BYTES]);
        let mv = MembershipVector::from_string(&s).unwrap();
        assert_eq!(mv.to_bytes(), vec![255u8; model::IDENTIFIER_SIZE_BYTES]);

        // 32 bytes of random input
        let s = random_hex_str(model::IDENTIFIER_SIZE_BYTES);
        let mv = MembershipVector::from_string(&s).unwrap();
        let expected_bytes = hex::decode(s).unwrap();
        assert_eq!(expected_bytes, mv.to_bytes());

        // 31 bytes should be left-padded with zeros
        let s = random_hex_str(model::IDENTIFIER_SIZE_BYTES - 1);
        let mv = MembershipVector::from_string(&s).unwrap();
        let expected_bytes = hex::decode(s).unwrap();
        assert_eq!(0, mv.to_bytes()[0]);
        assert_eq!(expected_bytes, mv.to_bytes()[1..]);

        // 33 bytes should return an error
        let s = random_hex_str(model::IDENTIFIER_SIZE_BYTES + 1);
        assert!(MembershipVector::from_string(&s).is_err());
    }

    #[test]
    fn test_common_bit_prefix() {
        // 32 bytes of zero
        let s_0 = vec![0u8; model::IDENTIFIER_SIZE_BYTES];
        let mv_0 = MembershipVector::from_bytes(&s_0).unwrap();

        // 32 bytes of ones
        let s_1 = vec![255u8; model::IDENTIFIER_SIZE_BYTES];
        let mv_1 = MembershipVector::from_bytes(&s_1).unwrap();

        // every membership vector should have complete common bits prefix with itself.
        assert_eq!(
            mv_0.common_prefix_bit(&mv_0),
            model::IDENTIFIER_SIZE_BYTES * 8
        );
        assert_eq!(
            mv_1.common_prefix_bit(&mv_1),
            model::IDENTIFIER_SIZE_BYTES * 8
        );

        // all zero and all one should not have any common bits prefix
        assert_eq!(mv_0.common_prefix_bit(&mv_1), 0);

        // first byte is 01111111 and the rest is all 1
        let first_bit_zero = [
            vec![127u8; 1],
            vec![255u8; model::IDENTIFIER_SIZE_BYTES - 1],
        ]
        .concat();
        let mv_01 = MembershipVector::from_bytes(&first_bit_zero).unwrap();
        // should have complete common prefix with itself
        assert_eq!(
            mv_01.common_prefix_bit(&mv_01),
            model::IDENTIFIER_SIZE_BYTES * 8
        );
        // should have zero common prefix with all 1s.
        assert_eq!(mv_1.common_prefix_bit(&mv_01), 0);
        assert_eq!(mv_01.common_prefix_bit(&mv_1), 0);
        // should have 1 common bit prefix with all 0s (only the first bit)
        assert_eq!(mv_0.common_prefix_bit(&mv_01), 1);
        assert_eq!(mv_01.common_prefix_bit(&mv_0), 1);

        // 00111111 11111111 ...
        // first two bits are zero and the rest is all 1
        let first_two_bits_zero =
            [vec![63u8; 1], vec![255u8; model::IDENTIFIER_SIZE_BYTES - 1]].concat();
        let mv_001 = MembershipVector::from_bytes(&first_two_bits_zero).unwrap();
        // should have complete common prefix with itself
        assert_eq!(
            mv_001.common_prefix_bit(&mv_001),
            model::IDENTIFIER_SIZE_BYTES * 8
        );
        // should have two bits common prefix with all 0s.
        assert_eq!(mv_001.common_prefix_bit(&mv_0), 2);
        assert_eq!(mv_0.common_prefix_bit(&mv_001), 2);
        // should have zero bits common prefix with all 1s.
        assert_eq!(mv_001.common_prefix_bit(&mv_1), 0);
        assert_eq!(mv_1.common_prefix_bit(&mv_001), 0);

        // 11111111 00000000 11111111 11111111 ...
        // first byte all ones; second byte all zeros; third and rest all ones
        let second_byte_all_zero = [
            vec![255u8; 1],
            vec![0u8; 1],
            vec![255u8; model::IDENTIFIER_SIZE_BYTES - 2],
        ]
        .concat();
        let mv_second_byte_all_zero = MembershipVector::from_bytes(&second_byte_all_zero).unwrap();
        // should have complete common prefix with itself.
        assert_eq!(
            mv_second_byte_all_zero.common_prefix_bit(&mv_second_byte_all_zero),
            model::IDENTIFIER_SIZE_BYTES * 8
        );
        // should have zero bits common prefix with all zeros
        assert_eq!(mv_second_byte_all_zero.common_prefix_bit(&mv_0), 0);
        assert_eq!(mv_0.common_prefix_bit(&mv_second_byte_all_zero), 0);
        // should have 8 bits (one complete byte) common prefix with all ones.
        assert_eq!(mv_second_byte_all_zero.common_prefix_bit(&mv_1), 8);
        assert_eq!(mv_1.common_prefix_bit(&mv_second_byte_all_zero), 8);

        // 11111111 10000000 11111111 11111111 ...
        let second_byte_128 = [
            vec![255u8; 1],
            vec![128u8; 1],
            vec![255u8; model::IDENTIFIER_SIZE_BYTES - 2],
        ]
        .concat();
        let mv_second_byte_128 = MembershipVector::from_bytes(&second_byte_128).unwrap();
        // should have complete common prefix with itself
        assert_eq!(
            mv_second_byte_128.common_prefix_bit(&mv_second_byte_128),
            model::IDENTIFIER_SIZE_BYTES * 8
        );
        // should have zero bits common prefix with all zeros;
        assert_eq!(mv_second_byte_128.common_prefix_bit(&mv_0), 0);
        assert_eq!(mv_0.common_prefix_bit(&mv_second_byte_128), 0);
        // should have 9 bits common prefix with all ones (first byte and first bit of the second byte):
        assert_eq!(mv_second_byte_128.common_prefix_bit(&mv_1), 8 + 1);
        assert_eq!(mv_1.common_prefix_bit(&mv_second_byte_128), 8 + 1);
        // should have 8 bits common prefix with the second_byte_all_zero
        assert_eq!(
            mv_second_byte_128.common_prefix_bit(&mv_second_byte_all_zero),
            8
        );
        assert_eq!(
            mv_second_byte_all_zero.common_prefix_bit(&mv_second_byte_128),
            8
        );

        // two random membership vectors that differ only in a random bit
        //
        let random_index = rand::rng().random_range(1..model::IDENTIFIER_SIZE_BYTES - 2);
        let left_bytes = random::bytes(random_index);
        let right_bytes = random::bytes(model::IDENTIFIER_SIZE_BYTES - random_index - 1);

        // sanity check; left and right must together fall one byte short
        assert_eq!(
            right_bytes.len() + left_bytes.len(),
            model::IDENTIFIER_SIZE_BYTES - 1
        );

        // Case 1: at random index; they differ at a fix bit position
        // in the random index; one is (1110 1111) (239) and the other one (1111 1111) (255)
        // at this byte they have 3 bits common prefix, so entirely they should have random_index * 8 + 3
        let mv_239 = MembershipVector::from_bytes(
            &[left_bytes.clone(), vec![239u8; 1], right_bytes.clone()].concat(),
        )
        .unwrap();
        let mv_255 = MembershipVector::from_bytes(
            &[left_bytes.clone(), vec![255u8; 1], right_bytes.clone()].concat(),
        )
        .unwrap();
        assert_eq!(mv_239.common_prefix_bit(&mv_255), random_index * 8 + 3);
        assert_eq!(mv_255.common_prefix_bit(&mv_239), random_index * 8 + 3);

        // Case 2: at random index; they differ at a random bit
        let byte_1 = random::bytes(1)[0];
        let random_bit_index = rand::rng().random_range(0..8);
        let byte_2 = byte_1 ^ (1 << random_bit_index);
        println!(
            "byte_1: {:8b}, byte_2: {:8b}, random_bit_index: {}, random_index: {}",
            byte_1,
            byte_2,
            random_bit_index,
            random_index * 8
        );
        let mv_1 = MembershipVector::from_bytes(
            &[left_bytes.clone(), vec![byte_1], right_bytes.clone()].concat(),
        )
        .unwrap();
        let mv_2 = MembershipVector::from_bytes(
            &[left_bytes.clone(), vec![byte_2], right_bytes.clone()].concat(),
        )
        .unwrap();
        // common prefix length is random_index * 8 + (7 - random_bit_index):
        // random_index * 8 is the common prefix before the random byte
        // 7 - random_bit_index is the common prefix within the random byte; the first discrepancy
        // is at the random_bit_index, so the first 8 - random_bit_index bits are common prefix.
        // However, random-bit-index starts from 0, so we indeed have (8 - (random_bit_index + 1)) bits common prefix.
        assert_eq!(
            mv_1.common_prefix_bit(&mv_2),
            random_index * 8 + (7 - random_bit_index)
        );
    }

    /// Test decomposing the prefix at a given pivot bit index. Both the membership vector and the pivot are fixed in this test.
    /// This is the minimum test case for the decompose_at_bit method.
    #[test]
    fn test_decompose_at_bit_fixed_mv_fixed_pivot() {
        let mv = MembershipVector::from_string(
            "a738f14dc0750b7f7e4c418fdda32c424e9ecf7280892d14648e405466a76f29",
        )
        .unwrap();
        let (left, pivot, right) = mv.decompose_at_bit(40);
        assert_eq!(left, "a738f14dc0");
        assert_eq!(pivot, "01110101");
        assert_eq!(
            right,
            "0b7f7e4c418fdda32c424e9ecf7280892d14648e405466a76f29"
        );
    }

    /// Test decomposing the prefix at a given pivot bit index. The membership vector is fixed in this test.
    #[test]
    fn test_decompose_at_bit_exhaustive_pivot() {
        let mv = MembershipVector::from_string(
            "a738f14dc0750b7f7e4c418fdda32c424e9ecf7280892d14648e405466a76f29",
        )
        .unwrap();

        for p in 0..model::IDENTIFIER_SIZE_BYTES * 8 - 1 {
            assert_valid_decompose(&mv, p, mv.decompose_at_bit(p));
        }
    }

    /// Test decomposing the prefix at a given pivot bit index. The membership vector is random in this test it tries 1000 random membership
    /// vectors, and for each membership vector it tests all combinations of pivots from 0 to 255.
    #[test]
    fn test_decompose_at_bit_exhaustive_pivot_random_mvs() {
        for _ in 0..1000 {
            let mv = random_membership_vector();

            for p in 0..model::IDENTIFIER_SIZE_BYTES * 8 - 1 {
                assert_valid_decompose(&mv, p, mv.decompose_at_bit(p));
            }
        }
    }

    fn assert_valid_decompose(
        mv: &MembershipVector,
        pivot_index: usize,
        (left, pivot, right): (String, String, String),
    ) {
        let expected_left = hex::encode(&mv.to_bytes()[0..pivot_index / 8]);
        let expected_pivot = format!("{:08b}", mv.to_bytes()[pivot_index / 8]);
        let expected_right = hex::encode(&mv.to_bytes()[pivot_index / 8 + 1..]);

        assert_eq!(left, expected_left, "p: {pivot_index}");
        assert_eq!(pivot, expected_pivot, "p: {pivot_index}");
        assert_eq!(right, expected_right, "p: {pivot_index}");
    }
}
