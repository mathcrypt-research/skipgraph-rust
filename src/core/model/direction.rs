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
/// putting it on the wire, and three cases cover every message.
///
/// 1. **A request.** The sender sends the value unchanged, and the receiver writes the slot it
///    names.
/// 2. **A forwarded request.** Each hop passes the value on unchanged. The next hop writes the
///    same direction of its own table, so the value still names the correct slot.
/// 3. **A reply that reports a write.** The sender wrote a slot in its own table, so it inverts
///    the value with [`Direction::opposite`] before replying. The receiver's matching slot is the
///    mirror of that one.
///
/// Not every reply falls under the third case. A reply that only repeats back the direction the
/// request asked about carries the value unchanged. The responder read that slot of its own table
/// and reports what it found there. The requester writes nothing from the value. Inverting it
/// would name a slot the responder never read.
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
