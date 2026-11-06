//! Checks that need the stand-in values: where each variable ended up after
//! Compose interpolated the file, and secrets written in as plain text.
//!
//! These only make sense for a compose file normalized with
//! [`Placement::stand_in`]; zendiloOS runs them on such a pass before it
//! interpolates the administrator's own values.

use serde_json::Value;

use crate::placement::{STAND_IN_PGID, STAND_IN_PUID, STAND_IN_ROOT, STAND_IN_SETTING, STAND_IN_TZ};
use crate::{Manifest, Placement, Problem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Setting,
    Account,
    TimeZone,
    Path,
    Port,
}

impl Kind {
    /// Where values of this kind may end up; `*` is any key or index.
    fn allowed(self) -> &'static [&'static [&'static str]] {
        const ENVIRONMENT: &[&str] = &["services", "*", "environment", "*"];
        match self {
            Self::Setting => &[
                ENVIRONMENT,
                &["services", "*", "command", "*"],
                &["services", "*", "entrypoint", "*"],
                &["services", "*", "healthcheck", "test", "*"],
            ],
            Self::Account => &[ENVIRONMENT, &["services", "*", "user"]],
            Self::TimeZone => &[ENVIRONMENT],
            Self::Path => &[&["services", "*", "volumes", "*", "source"]],
            Self::Port => &[
                ENVIRONMENT,
                &["services", "*", "ports", "*", "published"],
                &["services", "*", "ports", "*", "target"],
                &["services", "*", "expose", "*"],
                &["services", "*", "command", "*"],
                &["services", "*", "entrypoint", "*"],
                &["services", "*", "healthcheck", "test", "*"],
            ],
        }
    }

    fn rule(self) -> &'static str {
        match self {
            Self::Setting => {
                "settings may only be used in `environment`, `command`, `entrypoint` or `healthcheck`"
            }
            Self::Account => "`PUID` and `PGID` may only be used in `environment` or `user`",
            Self::TimeZone => "`TZ` may only be used in `environment`",
            Self::Path => "`APP_DATA` and `SHARE_*` may only be used as the source of a bind mount",
            Self::Port => {
                "`PORT_*` may only be used in `ports`, `expose`, `environment`, `command`, `entrypoint` or `healthcheck`"
            }
        }
    }
}

/// Checks a compose file normalized with the stand-in values of `manifest`:
/// each variable is used only where it belongs (a setting the administrator
/// types in must not end up in `image` or a mount), and no password, secret,
/// token or key is written into the file as plain text.
pub fn check_variables(normalized: &Value, manifest: &Manifest) -> Vec<Problem> {
    let stand_in = Placement::stand_in(manifest);
    let ports: Vec<String> = stand_in
        .ports
        .values()
        .filter(|port| Placement::is_stand_in_port(**port))
        .map(u16::to_string)
        .collect();
    let mut problems = Vec::new();
    let mut path = Vec::new();
    walk(normalized, &mut path, &mut |path, text, is_key| {
        for kind in kinds_in(text, &ports) {
            let at = display(path);
            if is_key {
                problems.push(Problem::compose(at, "variables cannot be used in keys"));
            } else if !kind.allowed().iter().any(|pattern| matches(path, pattern)) {
                problems.push(Problem::compose(at, kind.rule()));
            }
        }
    });

    let services = normalized.get("services").and_then(Value::as_object).into_iter().flatten();
    for (name, service) in services {
        let environment = service.get("environment").and_then(Value::as_object).into_iter().flatten();
        for (key, value) in environment {
            let value = value.as_str().unwrap_or_default();
            if is_secret_name(key) && is_literal_secret(value) {
                problems.push(Problem::compose(
                    format!("services.{name}.environment.{key}"),
                    "a secret written into compose.yaml is the same on every NAS; \
                     declare a setting with `generate: true` (or one the administrator enters) and use it here",
                ));
            }
        }
    }
    problems
}

#[derive(Debug, Clone)]
enum Segment {
    Key(String),
    Index(usize),
}

fn walk(value: &Value, path: &mut Vec<Segment>, visit: &mut impl FnMut(&[Segment], &str, bool)) {
    match value {
        Value::String(text) => visit(path, text, false),
        // Compose writes container ports as numbers.
        Value::Number(number) => visit(path, &number.to_string(), false),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                path.push(Segment::Index(i));
                walk(item, path, visit);
                path.pop();
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                path.push(Segment::Key(key.clone()));
                visit(path, key, true);
                walk(item, path, visit);
                path.pop();
            }
        }
        _ => {}
    }
}

fn kinds_in(text: &str, ports: &[String]) -> Vec<Kind> {
    let mut kinds = Vec::new();
    if text.contains(STAND_IN_SETTING) {
        kinds.push(Kind::Setting);
    }
    if text.contains(STAND_IN_PUID) || text.contains(STAND_IN_PGID) {
        kinds.push(Kind::Account);
    }
    if text.contains(STAND_IN_TZ) {
        kinds.push(Kind::TimeZone);
    }
    if text.contains(STAND_IN_ROOT) {
        kinds.push(Kind::Path);
    }
    if text.split(|c: char| !c.is_ascii_digit()).any(|number| ports.iter().any(|port| port == number)) {
        kinds.push(Kind::Port);
    }
    kinds
}

fn matches(path: &[Segment], pattern: &[&str]) -> bool {
    path.len() == pattern.len()
        && path.iter().zip(pattern).all(|(segment, expected)| match segment {
            _ if *expected == "*" => true,
            Segment::Key(key) => key == expected,
            Segment::Index(_) => false,
        })
}

fn display(path: &[Segment]) -> String {
    let mut text = String::new();
    for segment in path {
        match segment {
            Segment::Key(key) if text.is_empty() => text.push_str(key),
            Segment::Key(key) => {
                text.push('.');
                text.push_str(key);
            }
            Segment::Index(i) => text.push_str(&format!("[{i}]")),
        }
    }
    text
}

/// Names that hold secrets: `DB_PASSWORD`, `JWT_SECRET`, `API_TOKEN`,
/// `SECRET_KEY_BASE`, `APP_KEY`; not `*_FILE`, which name a file instead.
fn is_secret_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    if name.ends_with("_FILE") {
        return false;
    }
    ["PASSWORD", "PASSWD", "SECRET", "TOKEN", "APIKEY", "API_KEY"].iter().any(|word| name.contains(word))
        || name == "KEY"
        || name.ends_with("_KEY")
}

/// A value that looks like a secret someone typed in: not empty, not a flag
/// or a number, not filled in from a setting.
fn is_literal_secret(value: &str) -> bool {
    let flag = matches!(value.to_ascii_lowercase().as_str(), "true" | "false" | "yes" | "no" | "on" | "off");
    !value.is_empty()
        && !flag
        && !value.chars().all(|c| c.is_ascii_digit())
        && !value.contains(STAND_IN_SETTING)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::manifest::tests::minimal;
    use crate::{Port, Setting, SettingKind};

    fn manifest() -> Manifest {
        let mut manifest = minimal();
        manifest.ports = vec![Port { id: "web".into(), label: "Web".into(), default: 8080, fixed: false }];
        manifest.settings = vec![Setting {
            id: "ADMIN_PASSWORD".into(),
            label: "Administrator password".into(),
            description: None,
            kind: SettingKind::Password,
            default: None,
            required: true,
            generate: false,
            length: None,
            alphabet: None,
        }];
        manifest
    }

    fn check(normalized: Value) -> Vec<String> {
        check_variables(&normalized, &manifest()).iter().map(ToString::to_string).collect()
    }

    #[test]
    fn variables_where_they_belong_pass() {
        let normalized = json!({ "services": { "app": {
            "image": "x",
            "user": "zendiloos-stand-in-puid:zendiloos-stand-in-pgid",
            "environment": {
                "ADMIN_PASSWORD": "zendiloos-stand-in-setting-ADMIN_PASSWORD",
                "TZ": "zendiloos-stand-in-tz",
                "BASE_URL": "http://localhost:61001/",
                "DB_PASSWORD_FILE": "/run/secret",
                "TOKEN_TTL": "3600"
            },
            "ports": [{ "target": 61001, "published": "61001" }],
            "volumes": [{ "type": "bind", "source": "/zendiloos-stand-in/app-data", "target": "/data" }]
        }}});
        assert_eq!(check(normalized), Vec::<String>::new());
    }

    #[test]
    fn variables_out_of_place_are_refused() {
        let normalized = json!({ "services": { "app": {
            "image": "zendiloos-stand-in-setting-ADMIN_PASSWORD",
            "labels": { "zendiloos-stand-in-tz": "x" },
            "working_dir": "/zendiloos-stand-in/app-data",
            "pids_limit": 61001
        }}});
        assert_eq!(
            check(normalized),
            [
                "compose.yaml: services.app.image: settings may only be used in `environment`, `command`, `entrypoint` or `healthcheck`",
                "compose.yaml: services.app.labels.zendiloos-stand-in-tz: variables cannot be used in keys",
                "compose.yaml: services.app.pids_limit: `PORT_*` may only be used in `ports`, `expose`, `environment`, `command`, `entrypoint` or `healthcheck`",
                "compose.yaml: services.app.working_dir: `APP_DATA` and `SHARE_*` may only be used as the source of a bind mount",
            ]
        );
    }

    #[test]
    fn secrets_in_plain_text_are_refused() {
        let normalized = json!({ "services": { "db": {
            "environment": { "POSTGRES_PASSWORD": "postgres", "JWT_SECRET": "changeme", "DISABLE_PASSWORD": "true" }
        }}});
        let problems = check(normalized);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems[0].starts_with("compose.yaml: services.db.environment.JWT_SECRET: a secret written")
        );
        assert!(
            problems[1]
                .starts_with("compose.yaml: services.db.environment.POSTGRES_PASSWORD: a secret written")
        );
    }

    #[test]
    fn secret_names() {
        for name in ["DB_PASSWORD", "jwt_secret", "API_TOKEN", "SECRET_KEY_BASE", "APP_KEY", "OPENAI_APIKEY"]
        {
            assert!(is_secret_name(name), "{name}");
        }
        for name in ["POSTGRES_PASSWORD_FILE", "KEYBOARD", "MONKEY_COUNT", "TZ"] {
            assert!(!is_secret_name(name), "{name}");
        }
    }
}
