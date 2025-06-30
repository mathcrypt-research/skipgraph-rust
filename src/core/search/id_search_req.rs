use crate::core::lookup::lookup_table::LookupTableLevel;
use crate::core::model::direction::Direction;
use crate::core::Identifier;

pub struct IdentifierSearchRequest {
    pub target: Identifier,
    pub level: LookupTableLevel,
    pub direction: Direction,
}

impl IdentifierSearchRequest {
    pub fn new(target: Identifier, level: LookupTableLevel, direction: Direction) -> Self {
        IdentifierSearchRequest {
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
