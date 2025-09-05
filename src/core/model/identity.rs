use crate::core::{Address, Identifier, MembershipVector};

/// Identity is an immutable struct that represents a node's identity in the network (ID, MembershipVector, Address).
#[derive(Copy, Debug, PartialEq)]
pub struct Identity {
    id: Identifier,
    mem_vec: MembershipVector,
    address: Address, // network address of the node
}

#[allow(useless_deprecated)]
impl Clone for Identity {
    #[deprecated(note = "This type is Copy; prefer implicit copying instead of .clone()")]
    fn clone(&self) -> Self {
        *self
    }
}

impl Identity {
    /// Create a new Identity
    pub fn new(id: &Identifier, mem_vec: &MembershipVector, address: Address) -> Identity {
        Identity {
            id: *id,
            mem_vec: *mem_vec,
            address,
        }
    }

    /// Get the identifier of the node
    pub fn id(&self) -> &Identifier {
        &self.id
    }

    /// Get the membership vector of the node
    pub fn mem_vec(&self) -> &MembershipVector {
        &self.mem_vec
    }

    /// Get the address of the node
    pub fn address(&self) -> Address {
        self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::{random_identifier, random_membership_vector};
    use crate::core::Address;

    #[test]
    fn test_identity() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let address = Address::new("localhost", "1234");
        let identity = Identity::new(&id, &mem_vec, address);
        assert_eq!(identity.id(), &id);
        assert_eq!(identity.mem_vec(), &mem_vec);
        assert_eq!(identity.address(), address);
    }
}
