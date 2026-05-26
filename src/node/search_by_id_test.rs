use super::base_node::BaseNode;
use crate::core::model::direction::Direction;
use crate::core::model::identity::Identity;
use crate::core::testutil::fixtures::{
    random_address, random_identifier, random_identifier_greater_than,
    random_lookup_table_with_extremes, random_membership_vector, span_fixture,
};
use crate::core::{IdSearchReq, Identifier, LookupTable, LOOKUP_TABLE_LEVELS};
use crate::network::{Event, EventProcessorCore, NetworkMock};
use crate::node::core::BaseCore;
use std::sync::Arc;
use unimock::*;

/// Verifies the node, acting as an `EventProcessor`, relays an
/// `IdSearchRequest` event to the expected neighbor via the network.
#[test]
fn test_search_by_id_networking_integration_relay() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);
    let target = random_identifier();

    // Put a candidate in the left direction at level 0 so the local pick is
    // not self — forcing the relay branch.
    let safe_neighbor = random_identifier_greater_than(&target);
    lt.update_entry(
        Identity::new(
            &safe_neighbor,
            &random_membership_vector(),
            random_address(),
        ),
        0,
        Direction::Left,
    )
    .expect("failed to update entry in lookup table");

    let node_id = random_identifier();
    let search_request = IdSearchReq::new(node_id, target, 0, Direction::Left);
    let request_event = Event::IdSearchRequest(search_request);

    let (expected_lvl, expected_identity) = lt
        .left_neighbors()
        .unwrap()
        .into_iter()
        .filter(|(l, id)| *l <= search_request.level() && id.id() >= search_request.target())
        .min_by_key(|(_, id)| *id.id())
        .unwrap();

    let mock_net = Unimock::new((
        NetworkMock::register_processor
            .each_call(matching!(_))
            .answers(&|_, _| Ok(())),
        NetworkMock::send_event
            .each_call(matching!(_))
            .answers_arc(Arc::new(
                move |_, id: Identifier, event: Event| match event {
                    Event::IdSearchRequest(req) => {
                        assert_eq!(req.level(), expected_lvl);
                        assert_eq!(id, *expected_identity.id());
                        Ok(())
                    }
                    _ => panic!("expected IdSearchRequest payload, got: {:?}", event),
                },
            ))
            .once(),
        NetworkMock::clone_box
            .each_call(matching!())
            .answers(&|mock| Box::new(mock.clone())),
    ));

    let core = Box::new(BaseCore::new(
        span_fixture(),
        node_id,
        random_membership_vector(),
        Box::new(lt.clone()),
    ));
    let node =
        BaseNode::new(span_fixture(), core, Box::new(mock_net)).expect("failed to create BaseNode");

    let origin_id = random_identifier();
    node.process_incoming_event(origin_id, request_event)
        .expect("failed to process request event");
}

/// Verifies the node, acting as an `EventProcessor`, responds with an
/// `IdSearchResponse` event to the originator when this node's id is equal
/// to the search target.
#[test]
fn test_search_by_id_networking_integration_target_is_this_node() {
    let lt = random_lookup_table_with_extremes(LOOKUP_TABLE_LEVELS);

    let origin_id = random_identifier();
    let node_id = random_identifier();

    let search_request = IdSearchReq::new(origin_id, node_id, 0, Direction::Left);
    let request_event = Event::IdSearchRequest(search_request);

    let mock_net = Unimock::new((
        NetworkMock::register_processor
            .each_call(matching!(_))
            .answers(&|_, _| Ok(())),
        NetworkMock::send_event
            .each_call(matching!(_))
            .answers_arc(Arc::new(
                move |_, id: Identifier, event: Event| match event {
                    Event::IdSearchResponse(res) => {
                        assert_eq!(
                            id, origin_id,
                            "expected result to be to the originator's identifier"
                        );
                        assert_eq!(
                            *res.result(),
                            node_id,
                            "expected result to be the node's identifier"
                        );
                        Ok(())
                    }
                    _ => panic!("expected IdSearchResponse payload, got: {:?}", event),
                },
            ))
            .once(),
        NetworkMock::clone_box
            .each_call(matching!())
            .answers(&|mock| Box::new(mock.clone())),
    ));

    let core = Box::new(BaseCore::new(
        span_fixture(),
        node_id,
        random_membership_vector(),
        Box::new(lt.clone()),
    ));
    let node =
        BaseNode::new(span_fixture(), core, Box::new(mock_net)).expect("failed to create BaseNode");

    let outer_origin_id = random_identifier();
    node.process_incoming_event(outer_origin_id, request_event)
        .expect("failed to process request event");
}
