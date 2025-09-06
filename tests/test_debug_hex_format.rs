use skipgraph::core::{Identifier, MembershipVector, Address};
use skipgraph::core::model::identity::Identity;

#[test]
fn test_identifier_debug_shows_hex_not_raw_bytes() {
    // Test data from the GitHub issue example
    let id_bytes = [69, 11, 103, 102, 141, 75, 166, 128, 3, 116, 40, 7, 102, 211, 4, 44, 26, 234, 34, 150, 21, 99, 236, 216, 142, 116, 70, 33, 143, 133, 174, 83];
    let identifier = Identifier::from_bytes(&id_bytes).unwrap();
    
    let debug_output = format!("{:?}", identifier);
    let expected_hex = hex::encode(id_bytes);
    
    // Should show hex format, not raw byte array
    assert_eq!(debug_output, expected_hex);
    assert_eq!(debug_output, "450b67668d4ba6800374280766d3042c1aea22961563ecd88e7446218f85ae53");
    
    // Should NOT contain raw byte array format like "[69, 11, 103, ...]"
    assert!(!debug_output.contains("[69"));
    assert!(!debug_output.contains("69, 11"));
}

#[test]
fn test_membership_vector_debug_shows_hex_not_raw_bytes() {
    // Test data from the GitHub issue example
    let mem_vec_bytes = [81, 77, 192, 82, 129, 173, 87, 224, 233, 181, 134, 100, 170, 151, 59, 27, 241, 80, 96, 46, 54, 235, 57, 31, 97, 14, 136, 195, 63, 101, 157, 240];
    let mem_vec = MembershipVector::from_bytes(&mem_vec_bytes).unwrap();
    
    let debug_output = format!("{:?}", mem_vec);
    let expected_hex = hex::encode(mem_vec_bytes);
    
    // Should show hex format, not raw byte array
    assert_eq!(debug_output, expected_hex);
    assert_eq!(debug_output, "514dc05281ad57e0e9b58664aa973b1bf150602e36eb391f610e88c33f659df0");
    
    // Should NOT contain raw byte array format like "[81, 77, 192, ...]"
    assert!(!debug_output.contains("[81"));
    assert!(!debug_output.contains("81, 77"));
}

#[test]
fn test_identity_debug_shows_hex_format_for_components() {
    // Test data from the GitHub issue example
    let id_bytes = [69, 11, 103, 102, 141, 75, 166, 128, 3, 116, 40, 7, 102, 211, 4, 44, 26, 234, 34, 150, 21, 99, 236, 216, 142, 116, 70, 33, 143, 133, 174, 83];
    let mem_vec_bytes = [81, 77, 192, 82, 129, 173, 87, 224, 233, 181, 134, 100, 170, 151, 59, 27, 241, 80, 96, 46, 54, 235, 57, 31, 97, 14, 136, 195, 63, 101, 157, 240];
    
    let identifier = Identifier::from_bytes(&id_bytes).unwrap();
    let mem_vec = MembershipVector::from_bytes(&mem_vec_bytes).unwrap();
    let address = Address::new("localhost", "8080");
    let identity = Identity::new(&identifier, &mem_vec, address);
    
    let debug_output = format!("{:?}", identity);
    let expected_id_hex = hex::encode(id_bytes);
    let expected_mem_vec_hex = hex::encode(mem_vec_bytes);
    
    // Should contain hex representations of both identifier and membership vector
    assert!(debug_output.contains(&expected_id_hex));
    assert!(debug_output.contains(&expected_mem_vec_hex));
    assert!(debug_output.contains("450b67668d4ba6800374280766d3042c1aea22961563ecd88e7446218f85ae53"));
    assert!(debug_output.contains("514dc05281ad57e0e9b58664aa973b1bf150602e36eb391f610e88c33f659df0"));
    
    // Should NOT contain raw byte array formats
    assert!(!debug_output.contains("[69, 11"));
    assert!(!debug_output.contains("[81, 77"));
    assert!(!debug_output.contains("Identifier([69"));
    assert!(!debug_output.contains("MembershipVector([81"));
    
    // Should contain proper field names and structure
    assert!(debug_output.contains("Identity"));
    assert!(debug_output.contains("id:"));
    assert!(debug_output.contains("mem_vec:"));
    assert!(debug_output.contains("address:"));
}

#[test]
fn test_debug_format_consistency_with_display() {
    // Test that Debug format matches Display format for individual components
    let id_bytes = [255, 0, 128, 64, 32, 16, 8, 4, 2, 1, 0, 255, 128, 64, 32, 16, 8, 4, 2, 1, 255, 0, 128, 64, 32, 16, 8, 4, 2, 1, 0, 255];
    let identifier = Identifier::from_bytes(&id_bytes).unwrap();
    let mem_vec = MembershipVector::from_bytes(&id_bytes).unwrap();
    
    // Debug format should match Display format
    assert_eq!(format!("{:?}", identifier), format!("{}", identifier));
    assert_eq!(format!("{:?}", mem_vec), format!("{}", mem_vec));
    
    // Both should be hex encoded
    let expected_hex = hex::encode(id_bytes);
    assert_eq!(format!("{:?}", identifier), expected_hex);
    assert_eq!(format!("{}", identifier), expected_hex);
    assert_eq!(format!("{:?}", mem_vec), expected_hex);
    assert_eq!(format!("{}", mem_vec), expected_hex);
}

#[test]
fn test_various_byte_patterns_show_as_hex() {
    // Test edge cases: all zeros
    let all_zeros = [0u8; 32];
    let id_zeros = Identifier::from_bytes(&all_zeros).unwrap();
    let debug_zeros = format!("{:?}", id_zeros);
    assert_eq!(debug_zeros, "0000000000000000000000000000000000000000000000000000000000000000");
    assert!(!debug_zeros.contains("[0,"));
    
    // Test edge cases: all 255s
    let all_255s = [255u8; 32];
    let id_255s = Identifier::from_bytes(&all_255s).unwrap();
    let debug_255s = format!("{:?}", id_255s);
    assert_eq!(debug_255s, "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
    assert!(!debug_255s.contains("[255,"));
    
    // Test mixed pattern
    let mixed = [0, 255, 128, 64, 32, 16, 8, 4, 2, 1, 170, 85, 240, 15, 204, 51, 0, 255, 128, 64, 32, 16, 8, 4, 2, 1, 170, 85, 240, 15, 204, 51];
    let id_mixed = Identifier::from_bytes(&mixed).unwrap();
    let debug_mixed = format!("{:?}", id_mixed);
    assert_eq!(debug_mixed, hex::encode(mixed));
    assert!(!debug_mixed.contains("[0,"));
    assert!(!debug_mixed.contains("255,"));
}