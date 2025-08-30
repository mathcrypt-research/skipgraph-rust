mod base_node;
#[cfg(test)]
mod search_by_id_test;

use crate::core::{Identifier, IdSearchReq, IdSearchRes, MembershipVector};

/// Node is a trait that represents a single node in a skip graph.
#[allow(dead_code)]
pub trait Node {
    /// Returns the identifier of the node.
    fn get_identifier(&self) -> &Identifier;

    /// Returns the membership vector of the node.
    fn get_membership_vector(&self) -> &MembershipVector;

    /// Performs a search for the given identifier in the lookup table in the given direction and level.
    fn search_by_id(&self, req: &IdSearchReq)
        -> anyhow::Result<IdSearchRes>;

    /// Performs a search for the given membership vector in the lookup table in the given direction and level.
    #[allow(dead_code)]
    fn search_by_mem_vec(
        &self,
        req: &IdSearchReq,
    ) -> anyhow::Result<IdSearchRes>;

    /// Performs the join protocol hence joining the current node to the Skip Graph overlay network.
    /// The node will use the given introducer node to join the network.
    /// Join returns a error if the current node has already joined the network.
    #[allow(dead_code)]
    fn join(&self, introducer: Identifier) -> anyhow::Result<()>;
}
