use fixedstr::{str128, str8};

/// Represents a networking address; composed of host + port
#[derive(Copy, Debug, PartialEq)]
pub struct Address {
    host: str128, // up to 128 bytes (on stack)
    port: str8,   // up to 8 bytes (on stack)
}

#[allow(useless_deprecated)]
impl Clone for Address {
    #[deprecated(note = "This type is Copy; prefer implicit copying instead of .clone()")]
    fn clone(&self) -> Self {
        *self
    }
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
