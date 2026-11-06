//! What the validator asks Docker: to normalize a compose file, and what a
//! registry knows about an image.

use std::collections::BTreeSet;
use std::env;
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result};
use serde_json::Value;
use zendiloos_catalog::compose::{COMPOSE_FILE, ImageRef};
use zendiloos_catalog::{Architecture, Placement, Problem};

/// Variables of the caller that Docker itself needs; everything else is
/// cleared, so nothing from the machine leaks into the interpolation.
const PASSED_THROUGH: [&str; 6] =
    ["PATH", "HOME", "DOCKER_HOST", "DOCKER_CONFIG", "DOCKER_CONTEXT", "XDG_RUNTIME_DIR"];

const RATE_LIMITED: &str =
    "could not be checked: the registry turned the request away (rate limit); try again later";

/// `docker compose config --format json` for the app in `folder`, with the
/// variables of `placement`. The inner error is Compose's own complaint.
pub fn normalize(folder: &Path, placement: &Placement) -> Result<Result<Value, String>> {
    let mut command = Command::new("docker");
    command
        .args(["compose", "--file", COMPOSE_FILE, "--env-file", "/dev/null"])
        .args(["--project-name", &placement.project, "config", "--format", "json"])
        .current_dir(folder)
        .env_clear()
        .envs(PASSED_THROUGH.iter().filter_map(|name| Some((name, env::var_os(name)?))))
        .envs(placement.variables());
    let output = run(&mut command, "docker compose")?;
    if !output.status.success() {
        return Ok(Err(format!("Docker Compose cannot read it: {}", complaint(&output))));
    }
    let normalized =
        serde_json::from_slice(&output.stdout).context("reading what `docker compose config` wrote")?;
    Ok(Ok(normalized))
}

/// Asks the registry whether the image exists and has a variant for
/// every architecture the manifest lists.
pub fn check_image(service: &str, image: &ImageRef, architectures: &[Architecture]) -> Result<Vec<Problem>> {
    let at = format!("services.{service}.image");
    let manifest = match inspect(image, "{{json .Manifest}}")? {
        Ok(manifest) => manifest,
        Err(message) => {
            return Ok(vec![Problem::compose(at, unavailable(message))]);
        }
    };
    let platforms: BTreeSet<String> = match manifest.get("manifests").and_then(Value::as_array) {
        Some(manifests) => manifests.iter().filter_map(|entry| platform(entry.get("platform")?)).collect(),
        None => match inspect(image, "{{json .Image}}")? {
            Ok(config) => platform(&config).into_iter().collect(),
            Err(message) => {
                return Ok(vec![Problem::compose(at, unavailable(message))]);
            }
        },
    };
    Ok(architectures
        .iter()
        .filter(|architecture| !platforms.contains(&format!("linux/{}", architecture.as_str())))
        .map(|architecture| {
            let architecture = architecture.as_str();
            Problem::compose(
                &at,
                format!("has no linux/{architecture} variant, but app.yaml lists {architecture}"),
            )
        })
        .collect())
}

fn unavailable(message: String) -> String {
    if message == RATE_LIMITED { message } else { format!("the registry does not have it: {message}") }
}

fn inspect(image: &ImageRef, format: &str) -> Result<Result<Value, String>> {
    let mut command = Command::new("docker");
    command.args(["buildx", "imagetools", "inspect", &image.reference(), "--format", format]);
    let output = run(&mut command, "docker buildx imagetools")?;
    if !output.status.success() {
        return Ok(Err(complaint(&output)));
    }
    let value =
        serde_json::from_slice(&output.stdout).context("reading what `docker buildx imagetools` wrote")?;
    Ok(Ok(value))
}

fn platform(value: &Value) -> Option<String> {
    Some(format!("{}/{}", value.get("os")?.as_str()?, value.get("architecture")?.as_str()?))
}

fn run(command: &mut Command, what: &str) -> Result<Output> {
    command
        .output()
        .with_context(|| format!("running `{what}`; is Docker with Compose and Buildx installed?"))
}

/// The error Docker printed, without its log-style warnings.
fn complaint(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("toomanyrequests") || stderr.contains("429 Too Many Requests") {
        return RATE_LIMITED.into();
    }
    let lines: Vec<&str> =
        stderr.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with("time=")).collect();
    match lines.last() {
        Some(line) => line.trim_start_matches("ERROR: ").to_owned(),
        None => format!("it exited with {}", output.status),
    }
}
