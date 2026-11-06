//! The index zendiloOS downloads: every app with its manifest, its compose
//! file as written, and where its icon lies.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::Manifest;

/// Raised when the index changes in a way older zendiloOS versions cannot
/// read.
pub const INDEX_FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct Index {
    pub format: u32,
    pub apps: Vec<Entry>,
}

impl Index {
    pub fn new(mut apps: Vec<Entry>) -> Self {
        apps.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        Self { format: INDEX_FORMAT, apps }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub manifest: Manifest,
    /// `compose.yaml` as written; zendiloOS interpolates it on the NAS.
    pub compose: String,
    /// The icon, relative to the index.
    pub icon: String,
    /// Changes whenever any file of the app changes, so zendiloOS can offer
    /// an update even when only an image digest moved.
    pub digest: String,
}

impl Entry {
    pub fn new(manifest: Manifest, manifest_text: &str, compose: String, icon: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        for part in [manifest_text.as_bytes(), compose.as_bytes(), icon] {
            hasher.update((part.len() as u64).to_be_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        Self { icon: icon_path(&manifest.id), manifest, compose, digest: format!("sha256:{digest}") }
    }
}

/// Where the icon of an app lies, relative to the index.
pub fn icon_path(id: &str) -> String {
    format!("icons/{id}.png")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::{MINIMAL, minimal};

    #[test]
    fn entries_carry_the_manifest_flat_and_a_digest_of_all_files() {
        let entry = Entry::new(minimal(), MINIMAL, "services: {}\n".into(), b"png");
        let json = serde_json::to_value(Index::new(vec![entry.clone()])).unwrap();
        assert_eq!(json["format"], 1);
        assert_eq!(json["apps"][0]["id"], "demo");
        assert_eq!(json["apps"][0]["icon"], "icons/demo.png");
        assert!(json["apps"][0].get("ports").is_none(), "empty lists are left out");

        let other = Entry::new(minimal(), MINIMAL, "services: {}\n".into(), b"png2");
        assert_ne!(entry.digest, other.digest);
        assert!(entry.digest.starts_with("sha256:") && entry.digest.len() == 71);
    }
}
