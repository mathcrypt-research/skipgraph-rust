use crate::core::testutil::fixtures::{random_membership_vector, span_fixture};
use crate::core::{Identifier, LookupTable};
use crate::node::core::BaseCore;

/// Fills in a fixture span and a random membership vector for callers that
/// don't need to control those.
pub(crate) fn make_core(id: Identifier, lt: Box<dyn LookupTable>) -> BaseCore {
    BaseCore::new(span_fixture(), id, random_membership_vector(), lt)
}
