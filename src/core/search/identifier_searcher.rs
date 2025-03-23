use crate::core::model::identity::Identity;
use crate::core::lookup::lookup_table::LookupTable;
use crate::core::search::id_search_req::IdentifierSearchRequest;
use crate::core::search::id_search_res::IdentifierSearchResult;

trait IdentifierSearcher {
    /// Performs the search for given identifier in the lookup table in the given direction and level.
    /// Essentially looks for the first match in the direction for the given level and all levels below.
    /// The match is the first entry that is greater than or equal to the target identifier (for left direction), 
    /// or less than or equal to the target identifier (for right direction).
    /// Returns the search result.
    /// If the lookup table is empty in that direction, returns None.
    fn search_by_id(&self, lookup_table: &dyn LookupTable, search_req : IdentifierSearchRequest) -> anyhow::Result<IdentifierSearchResult>;
}

