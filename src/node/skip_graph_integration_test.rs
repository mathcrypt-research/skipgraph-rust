use super::base_node::BaseNode;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use crate::core::testutil::fixtures::{join_all_with_timeout, join_with_timeout, random_membership_vector, random_sorted_identifiers, span_fixture};
use crate::core::{Address, ArrayLookupTable, IdSearchReq, Identifier, LookupTable, MembershipVector, LOOKUP_TABLE_LEVELS};
use crate::core::model::search::RequestId;
use crate::network::mock::hub::NetworkHub;
use crate::network::Network;
use crate::node::core::BaseCore;

struct LocalSkipGraph {
    nodes: Vec<BaseNode>,
    lts: Vec<Box<dyn LookupTable>>,
    identifiers: Vec<Identifier>,
    mvs: Vec<MembershipVector>,
}

impl LocalSkipGraph {
    /// Builds a fully wired `n`-node skip graph for testing, sharing a single
    /// `NetworkHub`. Each node gets a unique sorted identifier and a random
    /// membership vector. Lookup tables are populated inline by running
    /// Algorithm 2 (insert/join, see `skip-graphs-paper.pdf`) — level 0 as a
    /// doubly-linked list, higher levels linking each node to its closest
    /// membership-vector prefix-match on either side. Sidesteps the placeholder
    /// `BaseNode::join` so tests can assert against a correctly-wired graph.
    fn new(n: usize) -> anyhow::Result<Self> {
        if n == 0 {
            return Err(anyhow::anyhow!("cannot create skip graph with 0 nodes"));
        }

        let hub = NetworkHub::new();
        let identifiers = random_sorted_identifiers(n);
        let mut nodes = Vec::with_capacity(n);
        let mut lts: Vec<Box<dyn LookupTable>> = Vec::with_capacity(n);

        for &id in &identifiers {
            let mem_vec = random_membership_vector();
            let lt: Box<dyn LookupTable> = Box::new(ArrayLookupTable::new());
            let network = NetworkHub::new_mock_network(hub.clone(), id)?;
            let core = Box::new(BaseCore::new(span_fixture(), id, mem_vec, lt.clone()));
            let node = BaseNode::new(span_fixture(), core, network.clone_box())?;
            nodes.push(node);
            lts.push(lt);
        }

        // Connects the nodes in a doubly-linked list at level zero, the first node does not have
        // a previous node and the last node does not have a next node.
        for (n_pair, lt_pair) in nodes.windows(2).zip(lts.windows(2)) {
            let this_id = Identity::new(
                n_pair[1].id(),
                n_pair[1].mem_vec(),
                Address::new("localhost", "0"),
            );
            let prev_id = Identity::new(
                n_pair[0].id(),
                n_pair[0].mem_vec(),
                Address::new("localhost", "0"),
            );

            lt_pair[0].update_entry(this_id, 0, Direction::Right)?;
            lt_pair[1].update_entry(prev_id, 0, Direction::Left)?;
        }

        for i in 1..n {
            let mut loop_start = i - 1; // exclude i from considering for its own left neighbor
            for level in 1..LOOKUP_TABLE_LEVELS {
                let mut neighbor_idx: Option<usize> = None;

                // moves leftward to find a neighbor at the given level
                for j in (0..=loop_start).rev() {
                    // Invariant: loop_start < i, so j < i throughout — no self-link possible.
                    if nodes[i].mem_vec().common_prefix_bit(nodes[j].mem_vec()) >= level {
                        let id_j = Identity::new(
                            nodes[j].id(),
                            nodes[j].mem_vec(),
                            Address::new("localhost", "0"),
                        );
                        let id_i = Identity::new(
                            nodes[i].id(),
                            nodes[i].mem_vec(),
                            Address::new("localhost", "0"),
                        );
                        lts[i].update_entry(id_j, level, Direction::Left)?;
                        lts[j].update_entry(id_i, level, Direction::Right)?;
                        neighbor_idx = Some(j);
                        break;
                    }
                }
                match neighbor_idx {
                    // if a neighbor was found, we continue to search at the next level from the same node
                    Some(j) => loop_start = j,
                    // if no neighbor was found, we stop searching at any other level, as we cannot find at least 'level'-bit common prefix,
                    // hence we cannot find > 'level'-bit common prefix for any other level.
                    None => break,
                }
            }
        }

        let mvs = nodes.iter().map(|n| *n.mem_vec()).collect();
        Ok(LocalSkipGraph {
            nodes,
            lts,
            identifiers,
            mvs,
        })
    }
}

/// For every (node, level) pair, asserts the Left/Right lookup-table entries
/// match the closest predecessor/successor whose membership vector shares at
/// least `level` bits with the node's — computed independently from `mvs`.
#[test]
fn test_lookup_tables_validity() {
    let sg = LocalSkipGraph::new(256).expect("failed to create skip graph");

    for (i, lt) in sg.lts.iter().enumerate() {
        for level in 0..LOOKUP_TABLE_LEVELS {
            // find the max j < i: common_prefix_bit(m_i, m_j) ≥ level
            let expected_left: Option<usize> = (0..i)
                .rev()
                .find(|&j| sg.mvs[i].common_prefix_bit(&sg.mvs[j]) >= level);

            let expected_left_neighbor_id = expected_left.map(|j| *sg.nodes[j].id());
            let actual_left_neighbor_id = lt
                .get_entry(level, Direction::Left)
                .expect("get_entry should never error")
                .map(|identity| *identity.id());
            assert_eq!(
                actual_left_neighbor_id, expected_left_neighbor_id,
                "left lookup table entry is not valid"
            );

            // find the min j > i: common_prefix_bit(m_i, m_j) >= level
            let expected_right: Option<usize> =
                (i + 1..sg.nodes.len()).find(|&j| sg.mvs[i].common_prefix_bit(&sg.mvs[j]) >= level);
            let expected_right_neighbor_id = expected_right.map(|j| *sg.nodes[j].id());
            let actual_right_neighbor_id = lt
                .get_entry(level, Direction::Right)
                .expect("get_entry should never error")
                .map(|identity| *identity.id());
            assert_eq!(
                actual_right_neighbor_id, expected_right_neighbor_id,
                "right lookup table entry is not valid"
            );
        }
    }
}

#[test]
fn test_skip_graph_edge_cases() {
    let sg = LocalSkipGraph::new(1).expect("failed to create single-node skip graph");
    assert_eq!(sg.nodes.len(), 1);
    assert_eq!(sg.identifiers.len(), 1);
    assert_eq!(sg.mvs.len(), 1);
    assert_eq!(sg.lts.len(), 1);

    assert!(
        LocalSkipGraph::new(0).is_err(),
        "creating an empty skip graph should fail"
    );
}

#[test]
fn test_skip_graph_search_by_id() {
    let sg = LocalSkipGraph::new(8).expect("failed to initialize a local skip graph");
    let origin_node = sg.nodes[0].clone();
    let target_id = sg.identifiers[7];

    let handle = std::thread::spawn(move || {
        let id_search_req = IdSearchReq::new(RequestId::random(), *origin_node.id(), target_id, LOOKUP_TABLE_LEVELS - 1, Direction::Right);
        let result = origin_node.search_by_id(&id_search_req).expect("failed to search by id");
        assert_eq!(*result.result(), target_id);
    });

    join_with_timeout(handle, std::time::Duration::from_secs(10))
        .expect("search_by_id did not complete within timeout (likely deadlocked)");
}

#[test]
fn test_skip_graph_search_by_id_concurrent() {
    let sg = LocalSkipGraph::new(8).expect("failed to initialize a local skip graph");
    let origin_node = sg.nodes[0].clone();

    let mut handles = Vec::with_capacity(8);
    for i in 0..8 {
        let origin_node = origin_node.clone();
        let target_id = sg.identifiers[i];
        let handle = std::thread::spawn(move || {
            let id_search_req = IdSearchReq::new(RequestId::random(), *origin_node.id(), target_id, LOOKUP_TABLE_LEVELS - 1, Direction::Right);
            let result = origin_node.search_by_id(&id_search_req).expect("failed to search by id");
            assert_eq!(*result.result(), target_id);
        });
        handles.push(handle);
    }

    join_all_with_timeout(handles.into_boxed_slice(), std::time::Duration::from_secs(10))
        .expect("search_by_id did not complete within timeout (likely deadlocked)");
}

