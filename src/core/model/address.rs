use fixedstr::{str128, str8};
use std::fmt::Debug;

/// Represents a networking address; composed of host + port
#[derive(Copy, Clone, PartialEq)]
pub struct Address {
    host: str128, // up to 128 bytes (on stack)
    port: str8,   // up to 8 bytes (on stack)
}

impl Address {
    /// Create a new Address
    pub fn new(host: &str, port: &str) -> Address {
        Address {
            host: str128::from(host),
            port: str8::from(port),
        }
    }

    /// Get the host
    pub fn host(&self) -> &str {
        self.host.as_str()
    }

    /// Get the port
    pub fn port(&self) -> &str {
        self.port.as_str()
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host(), self.port())
    }
}

impl Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address() {
        let address = Address::new("localhost", "1234");
        assert_eq!(address.host(), "localhost");
        assert_eq!(address.port(), "1234");
    }
}
