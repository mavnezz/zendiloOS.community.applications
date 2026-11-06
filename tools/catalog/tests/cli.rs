//! Runs the `catalog` binary as CI does. Needs Docker with the Compose plugin.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn catalog(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_catalog")).args(args).env_remove("GITHUB_ACTIONS").output().unwrap()
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/apps")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_catalog_itself_passes() {
    let apps = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
    let output = catalog(&["validate", "--offline", "--apps", apps.to_str().unwrap()]);
    assert!(output.status.success(), "{}{}", stdout(&output), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn an_app_that_breaks_the_policy_fails_with_reasons() {
    let output = catalog(&["validate", "--offline", "--apps", fixtures().to_str().unwrap(), "unsafe"]);
    let stdout = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    let expected = "\
✗ unsafe
    compose.yaml: services.app.environment.ADMIN_PASSWORD: a secret written into compose.yaml is the same on every NAS; declare a setting with `generate: true` (or one the administrator enters) and use it here
    compose.yaml: services.app.ports[0]: publish host ports as `${PORT_<ID>}:<container port>`, with the port declared under `ports` in app.yaml
    compose.yaml: services.app.privileged: privileged containers control the whole machine
    compose.yaml: services.app.volumes[0]: the Docker socket needs `permissions.docker_socket: true` in app.yaml, which the administrator confirms: whoever holds it controls every container and the NAS itself
    compose.yaml: services.app.volumes[1].source: the time zone comes from `TZ: ${TZ}` under `environment`
    compose.yaml: services.app.restart: set `unless-stopped`, so the app comes back after the NAS restarts (`\"no\"` for a job that runs once)
    app.yaml: ports[0]: is not published in compose.yaml as `${PORT_WEB}`
";
    let mut lines: Vec<&str> = stdout.lines().take_while(|line| !line.is_empty()).collect();
    let mut expected: Vec<&str> = expected.lines().collect();
    lines[1..].sort_unstable();
    expected[1..].sort_unstable();
    assert_eq!(lines, expected);
}

#[test]
fn a_broken_app_is_not_handed_to_docker() {
    let output = catalog(&["validate", "--offline", "--apps", fixtures().to_str().unwrap(), "broken"]);
    let stdout = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("`.env` does not belong here"), "{stdout}");
    assert!(stdout.contains("app.yaml:3: unknown field `colour`"), "{stdout}");
    assert!(stdout.contains("icon.png: is not a PNG image"), "{stdout}");
    assert!(stdout.contains("compose.yaml: services.app.env_file: reads a file from the NAS"), "{stdout}");
    assert!(!stdout.contains("Docker Compose"), "{stdout}");
}

#[test]
fn the_index_holds_every_app_and_its_icon() {
    let apps = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("index");
    let _ = std::fs::remove_dir_all(&out);
    let output = catalog(&["index", "--apps", apps.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stdout(&output));
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("index.json")).unwrap()).unwrap();
    assert_eq!(index["format"], 1);
    for app in index["apps"].as_array().unwrap() {
        assert!(out.join(app["icon"].as_str().unwrap()).is_file(), "{}", app["id"]);
        assert!(app["compose"].as_str().unwrap().contains("services:"));
    }
}
