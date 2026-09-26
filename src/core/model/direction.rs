use std::fmt::{Debug, Display};

/// Represents the direction of search and lookup table access in SkipGraph.
///
/// A `Direction` belongs to whoever holds the lookup table it indexes. [`Direction::Right`] always
/// means that holder's own right slot, the one holding neighbors with larger identifiers, and
/// [`Direction::Left`] always means its own left slot. The meaning is global, never relative to a
/// caller or to a hop.
///
/// # Directions carried in a network message
///
/// This section governs only a `Direction` that travels as a field of a request or a response. A
/// purely local use, such as indexing this node's own table through a
/// [`LookupTable`](crate::core::LookupTable) method, transforms nothing, and none of the rules
/// below apply to it.
///
/// On the wire the value is receiver-owned, so it names a slot in the table of the node that
/// receives the message. That is what decides whether the sender transforms the value before
/// putting it on the wire, and three shapes cover every case.
///
/// 1. A request naming a slot in the receiver's own table carries the value the sender chose,
///    untransformed.
/// 2. A request forwarded onward to a further node passes the value through unchanged, because the
///    next hop writes the same direction of its own table.
/// 3. A response reporting a write the sender just made in its own table, and so instructing the
///    receiver to make the reciprocal write, must invert the value with [`Direction::opposite`]
///    before sending, because the receiver's matching slot is the mirror of the one the sender
///    wrote.
///
/// A response that only echoes a query coordinate is not the third shape. It reports on the
/// responder's own table, and its requester writes no entry from the echoed value, so inverting it
/// would only make the coordinate disagree with the slot that was actually read.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    /// Returns the other variant.
    pub fn opposite(self) -> Direction {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

impl Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Left => write!(f, "Left"),
            Direction::Right => write!(f, "Right"),
        }
    }
}

impl Debug for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}
