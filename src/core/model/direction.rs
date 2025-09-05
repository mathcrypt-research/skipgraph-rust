/// Represents the direction of search and lookup table access in SkipGraph.
#[derive(Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Left,
    Right,
}

#[allow(useless_deprecated)]
impl Clone for Direction {
    #[deprecated(note = "This type is Copy; prefer implicit copying instead of .clone()")]
    fn clone(&self) -> Self {
        *self
    }
}
