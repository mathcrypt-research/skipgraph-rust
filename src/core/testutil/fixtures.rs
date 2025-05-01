
mod test_imports {
    pub use crate::core::model::direction::Direction;
    pub use crate::core::model::identity::Identity;
    pub use crate::core::testutil::random::random_hex_str;
    pub use crate::core::{model, Address, ArrayLookupTable, Identifier, LookupTable, MembershipVector};
    pub use rand::Rng;
}

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
    let mut lt = ArrayLookupTable::new();
    let ids = random_network_identities(2 * n);
    for i in 0..n {
        lt.update_entry(ids[i], i, Direction::Left).unwrap();
        lt.update_entry(ids[i + n], i, Direction::Right).unwrap();
    }
    lt
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
