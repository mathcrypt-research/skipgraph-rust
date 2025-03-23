use crate::core::model::identity::Identity;
use crate::core::search::id_search_req::IdentifierSearchRequest;

pub struct IdentifierSearchResult {
    pub req : IdentifierSearchRequest,
    pub result: Option<Identity>,    
}