//! The app format of the zendiloOS community catalog, and the policy every
//! app must meet.
//!
//! An app is a folder with three files: `app.yaml` (what the app is and what
//! it needs), `compose.yaml` (how Docker runs it) and `icon.png`. The checks
//! here are pure: they read text and bytes and return problems. Reading the
//! folder and asking Docker to interpolate the compose file is left to the
//! caller, so the catalog's validator and the zendiloOS agent can run the
//! very same checks, each with its own adapters.
//!
//! The order of the checks:
//!
//! 1. [`check_folder`]: the folder holds exactly the expected files.
//! 2. [`Manifest::parse`] and [`Manifest::check`]: the manifest.
//! 3. [`check_icon`]: the icon.
//! 4. [`compose::check_source`]: the compose file as written, before Docker
//!    reads it (keys that would make Docker read other files, variables).
//! 5. The caller runs `docker compose config --format json` with the
//!    variables of a [`Placement`].
//! 6. [`compose::check_normalized`]: what Docker would run, against the
//!    policy; with the stand-in placement also
//!    [`compose::check_variable_use`].

pub mod compose;
mod icon;
pub mod index;
mod manifest;
mod placement;
mod problem;

pub use icon::{ICON_FILE, MAX_ICON_BYTES, check_icon};
pub use manifest::{
    Access, Alphabet, Architecture, Category, DEVICE_KINDS, GRANTABLE_CAPABILITIES, MANIFEST_FILE, Manifest,
    Permissions, Port, RESERVED_HOST_PORTS, Setting, SettingKind, Share, Translation, Web, WebScheme,
    device_kind,
};
pub use placement::Placement;
pub use problem::{AppFile, Problem};

/// The files an app folder holds; nothing else is allowed in it, because
/// Docker would read some of them on its own (`.env`, `compose.override.yaml`).
pub const APP_FILES: [&str; 3] = [MANIFEST_FILE, compose::COMPOSE_FILE, ICON_FILE];

/// Checks the names in an app folder: all of [`APP_FILES`] and nothing else.
pub fn check_folder<S: AsRef<str>>(names: &[S]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for name in names {
        let name = name.as_ref();
        if !APP_FILES.contains(&name) {
            problems.push(Problem::folder(format!(
                "`{name}` does not belong here; an app folder holds only {}",
                APP_FILES.join(", ")
            )));
        }
    }
    for expected in APP_FILES {
        if !names.iter().any(|name| name.as_ref() == expected) {
            problems.push(Problem::folder(format!("`{expected}` is missing")));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_with_the_three_files_is_fine() {
        assert!(check_folder(&["icon.png", "app.yaml", "compose.yaml"]).is_empty());
    }

    #[test]
    fn extra_and_missing_files_are_reported() {
        let problems = check_folder(&["app.yaml", ".env", "compose.override.yaml"]);
        let messages: Vec<_> = problems.iter().map(|p| p.message.as_str()).collect();
        assert_eq!(messages.len(), 4, "{messages:?}");
        assert!(messages[0].starts_with("`.env` does not belong here"));
        assert!(messages[1].starts_with("`compose.override.yaml` does not belong here"));
        assert_eq!(messages[2], "`compose.yaml` is missing");
        assert_eq!(messages[3], "`icon.png` is missing");
    }
}
