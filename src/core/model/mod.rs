/// Represents the size of both an identifier and membership vector in bytes.
pub const IDENTIFIER_SIZE_BYTES: usize = 32;

pub mod address;
pub(crate) mod direction;
pub mod identifier;
pub mod identity;
pub mod memvec;
