//! Perceptual hash comparison for visual regression testing
//!
//! Uses image_hasher with gradient algorithm for stable perceptual hashing.
//! Compares images via Hamming distance with configurable threshold.

use anyhow::{Context, Result};
use image::DynamicImage;
use image_hasher::{HashAlg, HasherConfig, ImageHash};
use std::path::Path;

/// Hamming distance threshold for considering images "similar enough"
/// Distance <= 10 means images pass visual comparison
pub const HASH_PASS_THRESHOLD: u32 = 10;

/// Result of comparing two images
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareResult {
    /// Hamming distance between the two image hashes
    pub distance: u32,
    /// Whether the images are similar enough (distance <= HASH_PASS_THRESHOLD)
    pub passed: bool,
}

/// Compute perceptual hash of an image
///
/// Uses gradient algorithm for stable hashing across minor variations.
pub fn compute_hash(image_path: &Path) -> Result<ImageHash> {
    let img = image::open(image_path)
        .with_context(|| format!("Failed to open image: {}", image_path.display()))?;

    let hasher = HasherConfig::new().hash_alg(HashAlg::Gradient).to_hasher();

    Ok(hasher.hash_image(&img))
}

/// Compare two images using perceptual hashing
///
/// Returns CompareResult with Hamming distance and pass/fail status.
pub fn compare_images(img1: &Path, img2: &Path) -> Result<CompareResult> {
    let hash1 = compute_hash(img1)?;
    let hash2 = compute_hash(img2)?;

    let distance = hash1.dist(&hash2);
    let passed = distance <= HASH_PASS_THRESHOLD;

    Ok(CompareResult { distance, passed })
}

/// Helper to load image for hashing (used internally)
fn _load_image(path: &Path) -> Result<DynamicImage> {
    image::open(path).with_context(|| format!("Failed to load image: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("visual")
            .join("fixtures")
    }

    #[test]
    fn test_compute_hash_succeeds() {
        let baseline = fixtures_dir().join("baseline.png");
        let result = compute_hash(&baseline);
        assert!(result.is_ok(), "Should compute hash successfully");
    }

    #[test]
    fn test_compare_identical_images() {
        let baseline = fixtures_dir().join("baseline.png");
        let result = compare_images(&baseline, &baseline).unwrap();
        assert_eq!(
            result.distance, 0,
            "Identical images should have distance 0"
        );
        assert!(result.passed, "Identical images should pass");
    }
}
