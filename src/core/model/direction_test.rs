use crate::core::model::direction::Direction;

/// Both arms of the inverse map to the other variant, so a one-sided mirror such as
/// `Right => Right` fails here by assertion rather than only as an unbounded forwarding
/// chain that overflows the stack in a join round trip.
#[test]
fn test_direction_opposite_maps_each_variant_to_the_other() {
    assert_eq!(Direction::Left.opposite(), Direction::Right);
    assert_eq!(Direction::Right.opposite(), Direction::Left);
}
