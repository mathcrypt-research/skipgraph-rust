use rand::Rng;

/// Generate random bytes of the given size.
pub fn bytes(size: usize) -> Vec<u8> {
    let mut rng = rand::rng();
    (0..size).map(|_| rng.random::<u8>()).collect()
}

/// Generate a random hex string of the given size.
pub fn random_hex_str(size: usize) -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..size).map(|_| rng.random::<u8>()).collect();
    hex::encode(bytes)
}
