//! Reads one app folder and runs every check on it.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use zendiloos_catalog::compose::{self, COMPOSE_FILE, ImageRef};
use zendiloos_catalog::{
    AppFile, ICON_FILE, MANIFEST_FILE, Manifest, Placement, Problem, check_folder, check_icon,
};

use crate::docker;

/// An app that passed every check.
pub struct App {
    pub manifest: Manifest,
    pub manifest_text: String,
    pub compose: String,
    pub icon: Vec<u8>,
}

pub struct Checked {
    pub id: String,
    pub problems: Vec<Problem>,
    /// Present when there are no problems.
    pub app: Option<App>,
}

/// Runs every check on the app in `folder`; with `online`, also asks the
/// registries whether the image digests exist for every architecture.
pub fn check(folder: &Path, online: bool) -> Result<Checked> {
    let id = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut names = Vec::new();
    for entry in fs::read_dir(folder).with_context(|| format!("reading {}", folder.display()))? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    let mut problems = check_folder(&names);

    let manifest_text = read_text(folder, MANIFEST_FILE)?;
    let manifest = manifest_text.as_deref().and_then(|text| match Manifest::parse(text) {
        Ok(manifest) => {
            problems.extend(manifest.check(&id));
            Some(manifest)
        }
        Err(problem) => {
            problems.push(problem);
            None
        }
    });
    let icon = read_bytes(folder, ICON_FILE)?;
    if let Some(icon) = &icon {
        problems.extend(check_icon(icon));
    }
    let compose_text = read_text(folder, COMPOSE_FILE)?;

    if let Some(compose_text) = &compose_text {
        let source_problems = compose::check_source(compose_text, manifest.as_ref());
        // Docker reads the file only once it cannot make Compose read others.
        let readable = !source_problems.iter().any(|problem| problem.file == AppFile::Compose);
        problems.extend(source_problems);
        if let (true, Some(manifest)) = (readable, &manifest) {
            let placement = Placement::stand_in(manifest);
            match docker::normalize(folder, &placement)? {
                Ok(normalized) => {
                    problems.extend(compose::check_normalized(&normalized, manifest, &placement));
                    problems.extend(compose::check_variables(&normalized, manifest));
                    if online {
                        for (service, image) in compose::images(&normalized) {
                            if let Ok(image) = ImageRef::parse(&image) {
                                problems.extend(docker::check_image(
                                    &service,
                                    &image,
                                    &manifest.architectures,
                                )?);
                            }
                        }
                    }
                }
                Err(message) => problems.push(Problem::compose("", message)),
            }
        }
    }

    problems.sort_by_key(|problem| problem.file);
    let app = match (problems.is_empty(), manifest, manifest_text, compose_text, icon) {
        (true, Some(manifest), Some(manifest_text), Some(compose), Some(icon)) => {
            Some(App { manifest, manifest_text, compose, icon })
        }
        _ => None,
    };
    Ok(Checked { id, problems, app })
}

fn read_text(folder: &Path, name: &str) -> Result<Option<String>> {
    match read_bytes(folder, name)? {
        Some(bytes) => {
            let text = String::from_utf8(bytes)
                .with_context(|| format!("{name} in {} is not UTF-8", folder.display()))?;
            Ok(Some(text))
        }
        None => Ok(None),
    }
}

fn read_bytes(folder: &Path, name: &str) -> Result<Option<Vec<u8>>> {
    let path = folder.join(name);
    if !path.is_file() {
        return Ok(None);
    }
    fs::read(&path).map(Some).with_context(|| format!("reading {}", path.display()))
}
