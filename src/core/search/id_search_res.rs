use crate::core::model::identity::Identity;
use crate::core::search::id_search_req::IdentifierSearchRequest;

pub struct IdentifierSearchResult<T> {
    pub req : IdentifierSearchRequest,
    pub result: Option<Identity<T>>,
}