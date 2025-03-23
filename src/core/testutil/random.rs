pub fn bytes(size: usize) -> Vec<u8> {
    (0..size).map(|_| rand::random::<u8>()).collect()
}

pub fn random_hex_str(size: usize) -> String {
    hex::encode(bytes(size))
}
