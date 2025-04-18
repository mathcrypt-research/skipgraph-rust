/// Generate random bytes of the given size.
pub fn bytes(size: usize) -> Vec<u8> {
    (0..size).map(|_| rand::random::<u8>()).collect()
}

/// Generate a random hex string of the given size.
pub fn random_hex_str(size: usize) -> String {
    hex::encode(bytes(size))
}
