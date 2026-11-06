//! Checks on `compose.yaml` as written, before Docker reads it.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::manifest::{BUILT_IN_VARIABLES, yaml_error_message};
use crate::placement::STAND_IN_MARK;
use crate::{Manifest, Problem};

pub const MAX_COMPOSE_BYTES: usize = 64 * 1024;

/// Keys that make Compose read other files while it reads this one, before
/// any policy could look at what they contain.
const READS_FILES: [(&str, &str); 7] = [
    ("env_file", "reads a file from the NAS; set the variables under `environment`"),
    ("label_file", "reads a file from the NAS; set the labels under `labels`"),
    ("extends", "reads another compose file; write the service out in full"),
    ("include", "reads other compose files; put everything into compose.yaml"),
    ("build", "builds an image on the NAS; publish the image to a registry and refer to it by name"),
    ("secrets", "reads files from the NAS; pass values as settings"),
    ("configs", "reads files from the NAS; pass values as settings"),
];

/// Checks the compose file as written: that it parses and does not make
/// Compose read other files; with the manifest also that it uses only
/// variables zendiloOS provides, and every setting the manifest declares.
pub fn check_source(text: &str, manifest: Option<&Manifest>) -> Vec<Problem> {
    if text.len() > MAX_COMPOSE_BYTES {
        return vec![Problem::compose("", format!("is larger than {} KiB", MAX_COMPOSE_BYTES / 1024))];
    }
    let document: Value = match serde_saphyr::from_str(text) {
        Ok(document) => document,
        Err(error) => {
            let line = error.location().map(|location| location.line());
            return vec![Problem::compose("", yaml_error_message(&error)).at_line(line)];
        }
    };
    let Some(root) = document.as_object() else {
        return vec![Problem::compose("", "must be a mapping with `services`")];
    };

    let mut problems = Vec::new();
    if text.contains(STAND_IN_MARK) {
        problems.push(Problem::compose(
            "",
            format!("must not contain `{STAND_IN_MARK}`, the validator's marker"),
        ));
    }
    for key in root.keys() {
        if let Some(reason) = reads_files(key) {
            problems.push(Problem::compose(key.as_str(), reason));
        }
    }
    match root.get("services").and_then(Value::as_object) {
        Some(services) => {
            for (name, service) in services {
                for key in service.as_object().into_iter().flat_map(|service| service.keys()) {
                    if let Some(reason) = reads_files(key) {
                        problems.push(Problem::compose(format!("services.{name}.{key}"), reason));
                    }
                }
            }
        }
        None => problems.push(Problem::compose("services", "must describe at least one service")),
    }

    let Some(manifest) = manifest else {
        return problems;
    };
    let mut used = BTreeSet::new();
    collect_variables(&document, &mut used);
    let known = known_variables(manifest);
    for name in used.iter().filter(|name| !known.contains(name.as_str())) {
        problems.push(Problem::compose(
            "",
            format!("uses `${{{name}}}`, which zendiloOS does not provide; declare it under `settings` in app.yaml"),
        ));
    }
    for (i, setting) in manifest.settings.iter().enumerate() {
        if !used.contains(&setting.id) {
            problems.push(Problem::manifest(format!("settings[{i}].id"), "is not used in compose.yaml"));
        }
    }
    problems
}

fn reads_files(key: &str) -> Option<&'static str> {
    READS_FILES.iter().find(|(name, _)| *name == key).map(|(_, reason)| *reason)
}

fn known_variables(manifest: &Manifest) -> BTreeSet<String> {
    let mut known: BTreeSet<String> = BUILT_IN_VARIABLES.iter().map(|name| (*name).to_owned()).collect();
    known.extend(manifest.ports.iter().map(|port| Manifest::port_variable(&port.id)));
    known.extend(manifest.shares.iter().map(|share| Manifest::share_variable(&share.id)));
    known.extend(manifest.settings.iter().map(|setting| setting.id.clone()));
    known
}

/// Collects the names of all variables referenced in keys and strings.
fn collect_variables(value: &Value, used: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => variable_references(text, used),
        Value::Array(items) => items.iter().for_each(|item| collect_variables(item, used)),
        Value::Object(map) => {
            for (key, value) in map {
                variable_references(key, used);
                collect_variables(value, used);
            }
        }
        _ => {}
    }
}

/// The variables Compose interpolates in `text`: `$NAME` and `${NAME…}`,
/// including those inside a default (`${A:-$B}`); `$$` is a literal `$`.
fn variable_references(text: &str, used: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        match bytes.get(i + 1) {
            Some(b'$') => i += 2,
            Some(b'{') => i = read_name(bytes, i + 2, used),
            Some(c) if c.is_ascii_alphabetic() || *c == b'_' => i = read_name(bytes, i + 1, used),
            _ => i += 1,
        }
    }
}

fn read_name(bytes: &[u8], start: usize, used: &mut BTreeSet<String>) -> usize {
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if end > start {
        used.insert(String::from_utf8_lossy(&bytes[start..end]).into_owned());
    }
    end.max(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::minimal;
    use crate::{Setting, SettingKind};

    fn names(text: &str) -> Vec<String> {
        let mut used = BTreeSet::new();
        variable_references(text, &mut used);
        used.into_iter().collect()
    }

    #[test]
    fn variables_are_found_in_every_form() {
        assert_eq!(names("${A}/x $B ${C:-${D}} ${E:?no} $$F $"), ["A", "B", "C", "D", "E"]);
        assert_eq!(names("price: 5$"), Vec::<String>::new());
    }

    fn check(text: &str) -> Vec<String> {
        let mut manifest = minimal();
        manifest.settings.push(Setting {
            id: "DB_PASSWORD".into(),
            label: "Database password".into(),
            description: None,
            kind: SettingKind::Password,
            default: None,
            required: false,
            generate: true,
            length: None,
            alphabet: None,
        });
        check_source(text, Some(&manifest)).iter().map(ToString::to_string).collect()
    }

    #[test]
    fn known_variables_pass() {
        let text = "services:\n  db:\n    image: x\n    environment:\n      PASSWORD: ${DB_PASSWORD}\n      TZ: $TZ\n";
        assert_eq!(check(text), Vec::<String>::new());
    }

    #[test]
    fn unknown_variables_and_unused_settings_are_reported() {
        let text = "services:\n  db:\n    image: x\n    environment:\n      HOME: ${HOME}\n";
        assert_eq!(
            check(text),
            [
                "compose.yaml: uses `${HOME}`, which zendiloOS does not provide; declare it under `settings` in app.yaml",
                "app.yaml: settings[0].id: is not used in compose.yaml",
            ]
        );
    }

    #[test]
    fn keys_that_read_files_are_refused_even_through_anchors() {
        let text = "\
x-base: &base
  env_file: /etc/shadow
services:
  db:
    <<: *base
    image: x
    command: echo ${DB_PASSWORD}
include: [other.yaml]
";
        let problems = check(text);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].starts_with("compose.yaml: include: reads other compose files"));
        assert!(problems[1].starts_with("compose.yaml: services.db.env_file: reads a file"));
    }

    #[test]
    fn syntax_errors_name_the_line() {
        let problems = check("services:\n  db:\n    image: [x\n");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("compose.yaml:"), "{problems:?}");
    }

    #[test]
    fn the_stand_in_marker_is_refused() {
        let text = "services:\n  db:\n    image: x\n    command: zendiloos-stand-in ${DB_PASSWORD}\n";
        assert!(check(text)[0].contains("validator's marker"));
    }
}
