use crate::core::LookupTableLevel;
use crate::observability::labels::LevelBucket;

/// Levels are bucketed into the expected fixed ranges, including the boundary
/// values and the overflow tail.
#[test]
fn test_level_bucket_ranges() {
    let cases: &[(LookupTableLevel, LevelBucket)] = &[
        (0, LevelBucket::Low),
        (15, LevelBucket::Low),
        (16, LevelBucket::Medium),
        (63, LevelBucket::Medium),
        (64, LevelBucket::High),
        (255, LevelBucket::High),
        (256, LevelBucket::Overflow),
        (usize::MAX, LevelBucket::Overflow),
    ];

    for (level, expected) in cases {
        assert_eq!(
            LevelBucket::from_level(*level),
            *expected,
            "level {level} bucketed incorrectly"
        );
        // `From` must agree with `from_level`.
        assert_eq!(
            LevelBucket::from(*level),
            *expected,
            "from disagreed at {level}"
        );
    }
}
