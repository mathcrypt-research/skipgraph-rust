use crate::core::model::identity::Identity;
use crate::core::testutil::random::random_hex_str;
use crate::core::{model, Address, Identifier, MembershipVector};
use rand::Rng;

/// Generate a random identifier
pub fn random_identifier() -> Identifier {
    Identifier::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

/// Generate n random identifiers sorted in ascending order
pub fn random_sorted_identifiers(n: usize) -> Vec<Identifier> {
    let mut ids = (0..n)
        .map(|_| random_identifier())
        .collect::<Vec<Identifier>>();
    ids.sort();
    ids
}

/// Generate a random membership vector
pub fn random_membership_vector() -> MembershipVector {
    MembershipVector::from_string(&random_hex_str(model::IDENTIFIER_SIZE_BYTES)).unwrap()
}

/// Generate a random port
pub fn random_port() -> u16 {
    rand::thread_rng().gen_range(1024..=65535)
}

/// Generate a random address
pub fn random_address() -> Address {
    Address::new("localhost", &random_port().to_string())
}

/// Generate a random identity
pub fn random_identity() -> Identity {
    Identity::new(
        &random_identifier(),
        &random_membership_vector(),
        &random_address(),
    )
}

/// Generate n random identities
pub fn random_identities(n: usize) -> Vec<Identity> {
    (0..n).map(|_| random_identity()).collect()
}

#[cfg(test)]
mod test {
    use crate::core::model::identifier::ComparisonResult::CompareLess;

    /// Test random identifier generation, generates 100 random identifiers and checks if they are sorted in ascending order.
    #[test]
    fn test_random_identifiers() {
        let ids = super::random_sorted_identifiers(100);

        // ensures that the identifiers are sorted in ascending order
        ids.iter().skip(1).fold(&ids[0], |prev, curr|  {
            assert_eq!(CompareLess, prev.compare(curr).result());
            curr
        });
    }
}
