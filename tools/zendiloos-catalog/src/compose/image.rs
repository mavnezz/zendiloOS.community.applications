use serde_json::Value;

/// An image reference as Docker understands it: `name`, `name:tag`,
/// `name@sha256:…` or `name:tag@sha256:…`. Without a tag Docker pulls
/// `latest`; a digest is allowed but not required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub name: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
}

impl ImageRef {
    pub fn parse(image: &str) -> Result<Self, String> {
        if image.is_empty() || image.contains(char::is_whitespace) {
            return Err("must be an image reference such as `jellyfin/jellyfin:latest`".into());
        }
        let (reference, digest) = match image.split_once('@') {
            Some((reference, digest)) => (reference, Some(digest)),
            None => (image, None),
        };
        if let Some(digest) = digest {
            let hex = digest.strip_prefix("sha256:").unwrap_or_default();
            if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)) {
                return Err("a digest must be `sha256:` followed by 64 lower-case hex digits".into());
            }
        }
        // A colon after the last slash starts the tag; one before it belongs
        // to a registry port (`registry.example:5000/app`).
        let (name, tag) = match reference.rsplit_once(':') {
            Some((name, tag)) if !tag.contains('/') => (name, Some(tag)),
            _ => (reference, None),
        };
        if name.is_empty() || name.starts_with('/') || name.ends_with('/') {
            return Err("must name an image, such as `jellyfin/jellyfin`".into());
        }
        if let Some(tag) = tag {
            let valid = (1..=128).contains(&tag.len())
                && !tag.starts_with(['.', '-'])
                && tag.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
            if !valid {
                return Err(format!("`{tag}` is not a valid tag"));
            }
        }
        Ok(Self { name: name.to_owned(), tag: tag.map(str::to_owned), digest: digest.map(str::to_owned) })
    }

    /// The reference to ask a registry about: by digest when there is one,
    /// otherwise by tag (`latest` when none is named).
    pub fn reference(&self) -> String {
        match (&self.digest, &self.tag) {
            (Some(digest), _) => format!("{}@{digest}", self.name),
            (None, Some(tag)) => format!("{}:{tag}", self.name),
            (None, None) => format!("{}:latest", self.name),
        }
    }
}

/// The images of a normalized compose file, by service.
pub fn images(normalized: &Value) -> Vec<(String, String)> {
    let Some(services) = normalized.get("services").and_then(Value::as_object) else {
        return Vec::new();
    };
    services
        .iter()
        .filter_map(|(name, service)| Some((name.clone(), service.get("image")?.as_str()?.to_owned())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "sha256:fea863d0cd9ec445985d9c75fc6ed64e38a1d2766cf0c250a33d553f4fc1b771";

    #[test]
    fn tags_digests_and_neither_are_fine() {
        let image = ImageRef::parse("jellyfin/jellyfin:latest").unwrap();
        assert_eq!((image.tag.as_deref(), image.digest.as_deref()), (Some("latest"), None));
        assert_eq!(image.reference(), "jellyfin/jellyfin:latest");

        assert_eq!(ImageRef::parse("redis").unwrap().reference(), "redis:latest");

        let pinned =
            ImageRef::parse(&format!("ghcr.io/home-assistant/home-assistant:2026.5.4@{DIGEST}")).unwrap();
        assert_eq!(pinned.name, "ghcr.io/home-assistant/home-assistant");
        assert_eq!(pinned.tag.as_deref(), Some("2026.5.4"));
        assert_eq!(pinned.reference(), format!("ghcr.io/home-assistant/home-assistant@{DIGEST}"));
    }

    #[test]
    fn a_registry_port_is_not_a_tag() {
        let image = ImageRef::parse("registry.example:5000/app").unwrap();
        assert_eq!((image.name.as_str(), image.tag), ("registry.example:5000/app", None));
    }

    #[test]
    fn malformed_references_are_refused() {
        assert!(ImageRef::parse("").is_err());
        assert!(ImageRef::parse("jellyfin/jellyfin latest").is_err());
        assert!(ImageRef::parse("jellyfin/jellyfin:-x").unwrap_err().contains("not a valid tag"));
        assert!(ImageRef::parse("jellyfin/jellyfin@sha256:abc").unwrap_err().contains("64"));
    }
}
