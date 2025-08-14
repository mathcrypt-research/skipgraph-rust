use crate::core::{Identifier, IdentifierSearchRequest, IdentifierSearchResult, MembershipVector};

/// Node is a trait that represents a single node in a skip graph.
pub trait Node {
    /// Returns the identifier of the node.
    fn get_identifier(&self) -> &Identifier;

    /// Returns the membership vector of the node.
    fn get_membership_vector(&self) -> &MembershipVector;

    /// Performs a search for the given identifier in the lookup table in the given direction and level.
    fn search_by_id(&self, req: &IdentifierSearchRequest)
        -> anyhow::Result<IdentifierSearchResult>;

    /// Performs a search for the given membership vector in the lookup table in the given direction and level.
    fn search_by_mem_vec(
        &self,
        req: &IdentifierSearchRequest,
    ) -> anyhow::Result<IdentifierSearchResult>;

    /// Performs the join protocol hence joining the current node to the Skip Graph overlay network.
    /// The node will use the given introducer node to join the network.
    /// Join returns a error if the current node has already joined the network.
    fn join(&self, introducer: Self::Address) -> anyhow::Result<()>;
}
