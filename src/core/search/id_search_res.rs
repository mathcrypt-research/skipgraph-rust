use crate::core::lookup::lookup_table::LookupTableLevel;
use crate::core::Identifier;

pub struct IdentifierSearchResult {
    target: Identifier,
    level: LookupTableLevel,
    result: Identifier,
}

impl IdentifierSearchResult {
    pub fn new(target: Identifier, level: LookupTableLevel, result: Identifier) -> Self {
        IdentifierSearchResult {
            target,
            level,
            result,
        }
    }

    pub fn target(&self) -> &Identifier {
        &self.target
    }

    pub fn level(&self) -> LookupTableLevel {
        self.level
    }

    pub fn result(&self) -> &Identifier {
        &self.result
    }
}
