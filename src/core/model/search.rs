use crate::core::lookup::LookupTableLevel;
use crate::core::model::direction::Direction;
use crate::core::Identifier;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct RequestId {
    id: u128,
}

impl RequestId {
    pub fn random() -> Self {
        RequestId {
            id: rand::random::<u128>(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct IdSearchReq {
    req_id: RequestId,
    target: Identifier,
    origin: Identifier,
    level: LookupTableLevel,
    direction: Direction,
}

impl IdSearchReq {
    pub fn new(
        req_id: RequestId,
        origin: Identifier,
        target: Identifier,
        level: LookupTableLevel,
        direction: Direction,
    ) -> Self {
        IdSearchReq {
            req_id,
            target,
            origin,
            level,
            direction,
        }
    }

    pub fn target(&self) -> &Identifier {
        &self.target
    }

    pub fn level(&self) -> LookupTableLevel {
        self.level
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn origin(&self) -> &Identifier {
        &self.origin
    }

    pub fn req_id(&self) -> RequestId {
        self.req_id
    }
}

#[derive(Debug, Copy, Clone)]
pub struct IdSearchRes {
    /// The unique identifier of the search request across all nodes (randomly generated).
    request_id: RequestId,
    /// The identifier that is being searched for.
    target: Identifier,
    /// The level of the lookup table where the search was terminated at the current node.
    termination_level: LookupTableLevel,
    /// The identifier that was found during the search process at the current node.
    result: Identifier,
}

impl IdSearchRes {
    /// Constructs a new `IdentifierSearchResult` instance.
    ///
    /// # Parameters
    ///
    /// - `request_id`: A `RequestId` that uniquely identifies the search request across all nodes (randomly generated).
    /// - `target`: An `Identifier` representing the target element for the search operation in the lookup table of the current node.
    /// - `termination_level`: A `LookupTableLevel` that specifies the lookup level where the search was terminated at the current node.
    /// - `result`: An `Identifier` holding the result of the search operation at the current node.
    ///
    /// # Returns
    ///
    /// Returns a new `IdentifierSearchResult` instance.
    /// and `result` parameters.
    pub fn new(request_id: RequestId, target: Identifier, level: LookupTableLevel, result: Identifier) -> Self {
        IdSearchRes {
            request_id,
            target,
            termination_level: level,
            result,
        }
    }

    /// Returns a reference to the `target` field of the struct.
    pub fn target(&self) -> &Identifier {
        &self.target
    }

    /// Returns the level of the lookup table where the search was terminated at the current node.
    pub fn termination_level(&self) -> LookupTableLevel {
        self.termination_level
    }

    /// Returns the result of the search operation at the current node.
    pub fn result(&self) -> &Identifier {
        &self.result
    }
    
    /// Returns the request id of the search, the unique id that identifies the search request across all nodes.
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }
}
