//! `compose.yaml`: how Docker runs an app, and what it may do.
//!
//! The policy is checked on what `docker compose config --format json`
//! makes of the file, not on the file as written: Compose has already
//! interpolated the variables, merged anchors and turned every short form
//! into the long one, so there is exactly one way to say each thing. The
//! zendiloOS agent can then run that normalized form itself, so what is
//! checked is what runs.

mod image;
mod normalized;
mod source;
mod variables;

pub use image::{ImageRef, images};
pub use normalized::check_normalized;
pub use source::{MAX_COMPOSE_BYTES, check_source};
pub use variables::check_variables;

pub const COMPOSE_FILE: &str = "compose.yaml";

/// Whether `path` is `root` or lies below it. Both must be lexically clean.
pub(crate) fn is_below(path: &str, root: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

/// An absolute path with `.` and empty components removed; `None` for a
/// relative path or one with `..`.
pub(crate) fn clean_path(path: &str) -> Option<String> {
    if !path.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part => parts.push(part),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_cleaned_and_compared_by_component() {
        assert_eq!(clean_path("/a//b/./c/").as_deref(), Some("/a/b/c"));
        assert_eq!(clean_path("/a/../etc"), None);
        assert_eq!(clean_path("a/b"), None);
        assert!(is_below("/data/app", "/data/app"));
        assert!(is_below("/data/app/config", "/data/app"));
        assert!(!is_below("/data/app2", "/data/app"));
    }
}
