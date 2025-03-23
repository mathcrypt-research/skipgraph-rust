use crate::core::Identifier;
use crate::core::lookup::lookup_table::Level;
use crate::core::model::direction::Direction;

pub struct IdentifierSearchRequest {
    pub target : Identifier,
    pub level : Level,
    pub direction : Direction,
}