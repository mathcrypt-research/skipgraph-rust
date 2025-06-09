use crate::core::Identifier;
use crate::core::lookup::lookup_table::LookupTableLevel;
use crate::core::model::direction::Direction;

pub struct IdentifierSearchRequest {
    pub target : Identifier,
    pub level : LookupTableLevel,
    pub direction : Direction,
}