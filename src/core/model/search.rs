use crate::core::lookup::LookupTableLevel;
use crate::core::model::direction::Direction;
use crate::core::Identifier;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct Nonce {
    id: u128,
}

impl Nonce {
    pub fn random() -> Self {
        Nonce {
            id: rand::random::<u128>(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct IdSearchReq {
    /// The unique identifier of the search request across all nodes (randomly generated).
    pub nonce: Nonce,
    /// The identifier that is being searched for.
    pub target: Identifier,
    /// The identifier of the node that initiated the search.
    pub origin: Identifier,
    /// The level of the lookup table where the search is being performed.
    pub level: LookupTableLevel,
    /// The direction of the search.
    pub direction: Direction,
}

#[derive(Debug, Copy, Clone)]
pub struct IdSearchRes {
    /// The unique identifier of the search request across all nodes (randomly generated).
    pub nonce: Nonce,
    /// The identifier that is being searched for.
    pub target: Identifier,
    /// The level of the lookup table where the search was terminated at the current node.
    pub termination_level: LookupTableLevel,
    /// The identifier that was found during the search process at the current node.
    pub result: Identifier,
}
