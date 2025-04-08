use crate::core::{
    Identifier, IdentifierSearchRequest, IdentifierSearchResult, LookupTable, MembershipVector,
    Node,
};
use std::fmt;
use std::fmt::Formatter;
use std::sync::Arc;

/// LocalNode is a struct that represents a single node in the local implementation of the skip graph.

struct LocalNode {
    id: Identifier,
    mem_vec: MembershipVector,
    lt: Box<dyn LookupTable<Arc<LocalNode>>>,
}

impl Node for LocalNode {
    type Address = Arc<LocalNode>;

    fn get_identifier(&self) -> &Identifier {
        &self.id
    }

    fn get_membership_vector(&self) -> &MembershipVector {
        &self.mem_vec
    }

    fn get_address(&self) -> Self::Address {
        Arc::new(self.clone())
    }

    fn search_by_id(
        &self,
        req: &IdentifierSearchRequest,
    ) -> anyhow::Result<IdentifierSearchResult<Self::Address>> {
        todo!()
    }

    fn search_by_mem_vec(
        &self,
        req: &IdentifierSearchRequest,
    ) -> anyhow::Result<IdentifierSearchResult<Self::Address>> {
        todo!()
    }

    fn join(&self, introducer: Self::Address) -> anyhow::Result<()> {
        todo!()
    }
}

/// Implementing PartialEq for LocalNode to compare the id and membership vector.
/// This basically supports == operator for LocalNode.
/// The cardinal assumption is that the id and membership vector are unique for each node.
impl PartialEq for LocalNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.mem_vec == other.mem_vec
        // ignore lt for equality check as comparing trait objects is non-trivial
    }
}

impl fmt::Debug for LocalNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalNode")
            .field("id", &self.id)
            .field("mem_vec", &self.mem_vec)
            .finish()
    }
}

impl Clone for LocalNode {
    fn clone(&self) -> Self {
        LocalNode {
            id: self.id.clone(),
            mem_vec: self.mem_vec.clone(),
            lt: self.lt.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testutil::fixtures::{random_identifier, random_membership_vector};
    use crate::core::ArrayLookupTable;

    #[test]
    fn test_local_node() {
        let id = random_identifier();
        let mem_vec = random_membership_vector();
        let node = LocalNode {
            id: id.clone(),
            mem_vec: mem_vec.clone(),
            lt: Box::new(ArrayLookupTable::new()),
        };
        assert_eq!(node.get_identifier(), &id);
        assert_eq!(node.get_membership_vector(), &mem_vec);
        // TODO: implement get_address for LocalNode
        // assert_eq!(node.get_address(), &node);
    }
}
