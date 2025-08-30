use crate::core::lookup::LookupTableLevel;
use crate::core::model::direction::Direction;
use crate::core::Identifier;

#[derive(Debug, Clone, Copy)]
pub struct IdSearchReq {
    pub target: Identifier,
    pub level: LookupTableLevel,
    pub direction: Direction,
}

impl IdSearchReq {
    pub fn new(target: Identifier, level: LookupTableLevel, direction: Direction) -> Self {
        IdSearchReq {
            target,
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
}

/// A struct representing the result of an identifier search within lookup table of current node.
///
/// The `IdentifierSearchResult` struct is composed of three key components:
/// - The `target` identifier that was searched for in the lookup table of the current node.
/// - The `termination_level` of the lookup table where the identifier search was terminated at the current node.
/// - The `result` identifier that was found during the search process at the current node.
///
/// # Derives
///
/// This struct derives the `Debug` trait, enabling it to be formatted using the `{:?}` formatter
/// for debugging purposes.
#[derive(Debug, Clone, Copy)]
pub struct IdSearchRes {
    target: Identifier,
    termination_level: LookupTableLevel,
    result: Identifier,
}

impl IdSearchRes {
    /// Constructs a new `IdentifierSearchResult` instance.
    ///
    /// # Parameters
    ///
    /// - `target`: An `Identifier` representing the target element for the search operation in the lookup table of the current node.
    /// - `termination_level`: A `LookupTableLevel` that specifies the lookup level where the search was terminated at the current node.
    /// - `result`: An `Identifier` holding the result of the search operation at the current node.
    ///
    /// # Returns
    ///
    /// Returns a new `IdentifierSearchResult` instance populated with the provided `target`, `level`,
    /// and `result` parameters.
    pub fn new(target: Identifier, level: LookupTableLevel, result: Identifier) -> Self {
        IdSearchRes {
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
}