use crate::core::testutil::fixtures::{
    random_address, random_membership_vector, random_sorted_identifiers, span_fixture,
};
use crate::core::{ArrayLookupTable, Identifier, LookupTable};
use crate::network::mock::hub::NetworkHub;
use crate::network::Network;
use crate::node::base_node::BaseNode;
use crate::node::core::BaseCore;

/// Fills in a fixture span and a random membership vector for callers that
/// don't need to control those.
pub(crate) fn make_core(id: Identifier, lt: Box<dyn LookupTable>) -> BaseCore {
    BaseCore::new(span_fixture(), id, random_membership_vector(), lt)
}

/// Builds `n` nodes that share one mock network hub, so they can send real messages to each
/// other. Their identifiers are ascending, and their lookup tables start empty, so a caller
/// links them itself through whatever protocol it tests.
///
/// The returned vectors are parallel. Element `i` of each one belongs to the same node. Each
/// returned lookup table is a shallow clone of the one its node holds, so a write the node makes
/// is visible through it.
///
/// # Returns
/// The node identifiers in ascending order, the nodes, and each node's lookup table.
pub(crate) fn sorted_nodes_fixture(
    n: usize,
) -> (Vec<Identifier>, Vec<BaseNode>, Vec<ArrayLookupTable>) {
    let hub = NetworkHub::new();
    let ids = random_sorted_identifiers(n);
    let tables: Vec<ArrayLookupTable> = (0..n).map(|_| ArrayLookupTable::new()).collect();
    let nodes: Vec<BaseNode> = ids
        .iter()
        .zip(tables.iter())
        .map(|(&id, lt)| {
            let net = NetworkHub::new_mock_network(hub.clone(), id, random_address())
                .expect("failed to create a mock network");
            BaseNode::new(
                span_fixture(),
                Box::new(make_core(id, Box::new(lt.clone()))),
                net.clone_box(),
            )
            .expect("failed to create a node")
        })
        .collect();

    (ids, nodes, tables)
}
