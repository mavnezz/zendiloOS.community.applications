//! `app.yaml`: what an app is, and what it needs from the machine.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::Problem;

pub const MANIFEST_FILE: &str = "app.yaml";

/// Kinds of devices an administrator can hand to an app, by path, with what
/// they are for. A `*` stands for a number (`/dev/ttyUSB*` covers
/// `/dev/ttyUSB0`); any other path covers itself and what lies below it. Disks, partitions, memory and consoles are
/// not among them: they would hand over the NAS itself.
///
/// zendiloOS passes a device only when the machine has it, so an app that
/// can use a GPU or a Zigbee stick still runs on a machine without one.
pub const DEVICE_KINDS: [(&str, &str); 10] = [
    ("/dev/dri", "video acceleration (Intel Quick Sync, AMD and Intel VAAPI)"),
    ("/dev/net/tun", "VPN tunnels"),
    ("/dev/ttyUSB*", "USB serial adapters: Zigbee and Z-Wave sticks, UPS"),
    ("/dev/ttyACM*", "USB serial devices: Zigbee and Z-Wave sticks, 3D printers"),
    ("/dev/ttyAMA*", "serial ports of ARM boards"),
    ("/dev/ttyS*", "serial ports"),
    ("/dev/serial/by-id", "serial devices by their stable name"),
    ("/dev/video*", "cameras and capture cards"),
    ("/dev/snd", "sound cards"),
    ("/dev/apex_*", "Coral Edge TPUs"),
];

/// The kind (the first column of [`DEVICE_KINDS`]) a device path belongs
/// to, if any.
pub fn device_kind(path: &str) -> Option<&'static str> {
    DEVICE_KINDS.iter().map(|(kind, _)| *kind).find(|kind| match kind.strip_suffix('*') {
        Some(prefix) => {
            path.strip_prefix(prefix).is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        }
        None => crate::compose::is_below(path, kind),
    })
}

/// Capabilities beyond Docker's default set that an administrator can grant.
pub const GRANTABLE_CAPABILITIES: [&str; 2] = ["NET_ADMIN", "SYS_NICE"];

/// Host ports the NAS itself uses (SSH, HTTP/HTTPS for the interface,
/// NetBIOS, SMB, NFS with rpcbind, mDNS, WS-Discovery); no app may claim them.
pub const RESERVED_HOST_PORTS: [u16; 11] = [22, 80, 111, 137, 138, 139, 443, 445, 2049, 3702, 5353];

/// Variables zendiloOS always provides; settings cannot use these names.
pub(crate) const BUILT_IN_VARIABLES: [&str; 4] = ["APP_DATA", "PUID", "PGID", "TZ"];

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Equal to the folder name: lower-case letters, digits and hyphens.
    pub id: String,
    pub name: String,
    /// The version of the app itself, for apps whose images name one; most
    /// follow a moving tag such as `latest` and leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// One line, shown under the name.
    pub tagline: String,
    pub description: String,
    pub categories: Vec<Category>,
    /// Who makes the app (not who packaged it for the catalog).
    pub developer: String,
    pub website: String,
    /// Where the app's source code lives.
    pub source: String,
    /// SPDX expression of the app's licence.
    pub license: String,
    /// GitHub accounts that look after this entry in the catalog.
    pub maintainers: Vec<String>,
    pub architectures: Vec<Architecture>,
    /// Shown to the administrator before installing: first steps, default
    /// accounts, limitations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// The web interface zendiloOS opens for the app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<Web>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<Port>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shares: Vec<Share>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<Setting>,
    #[serde(default, skip_serializing_if = "Permissions::is_empty")]
    pub permissions: Permissions,
    /// Texts in other languages, by ISO 639-1 code; English is the default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub translations: BTreeMap<String, Translation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Ai,
    Backup,
    Communication,
    Development,
    Downloads,
    Files,
    Finance,
    Games,
    HomeAutomation,
    Media,
    Monitoring,
    Network,
    Photos,
    Productivity,
    Security,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    Amd64,
    Arm64,
}

impl Architecture {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Amd64 => "amd64",
            Self::Arm64 => "arm64",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Web {
    /// The id of a port under `ports`.
    pub port: String,
    #[serde(default = "root_path")]
    pub path: String,
    #[serde(default)]
    pub scheme: WebScheme,
}

fn root_path() -> String {
    "/".to_owned()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WebScheme {
    #[default]
    Http,
    Https,
}

/// A port the app offers on the host.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Port {
    /// Lower-case letters, digits and hyphens; the compose file publishes the
    /// port as `${PORT_<ID>}`.
    pub id: String,
    pub label: String,
    /// The host port zendiloOS proposes; the administrator may choose another.
    pub default: u16,
    /// The app listens on this port itself (host network), so zendiloOS can
    /// only check that it is free, not move it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
}

/// A share the administrator picks when installing; the compose file mounts
/// it as `${SHARE_<ID>}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Share {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    ReadOnly,
    ReadWrite,
}

/// A value the administrator enters, or zendiloOS generates, when
/// installing; the compose file uses it as `${<ID>}`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    /// Upper-case letters, digits and underscores.
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub kind: SettingKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// zendiloOS generates a random value once, when installing, instead of
    /// asking: for passwords that only the app's own services share.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generate: bool,
    /// How many characters a generated value has; 32 when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u8>,
    /// Which characters a generated value is made of; letters and digits
    /// when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alphabet: Option<Alphabet>,
}

/// The characters of a generated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Alphabet {
    /// `A–Z`, `a–z`, `0–9`: safe in connection strings, URLs and shells.
    Alphanumeric,
    /// `0–9`, `a–f`: for keys an app reads as hexadecimal.
    Hex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingKind {
    Text,
    Password,
    Number,
    Boolean,
}

/// Exceptions from the policy that an administrator confirms when
/// installing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Device paths of the kinds in [`DEVICE_KINDS`]: a whole kind such as
    /// `/dev/dri`, or one device such as `/dev/ttyUSB0`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<String>,
    /// From [`GRANTABLE_CAPABILITIES`], without the `CAP_` prefix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    /// Services may use `network_mode: host`: they see every interface of
    /// the machine and can listen on any free port.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub host_network: bool,
    /// Services may mount the Docker socket (`/var/run/docker.sock`): they
    /// can then start, stop and change any container, which is control over
    /// the whole NAS.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub docker_socket: bool,
}

impl Permissions {
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty() && self.capabilities.is_empty() && !self.host_network && !self.docker_socket
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Translation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Manifest {
    /// Reads `app.yaml`. Unknown fields are errors, so a typo does not go
    /// unnoticed.
    pub fn parse(text: &str) -> Result<Self, Problem> {
        serde_saphyr::from_str(text).map_err(|error| {
            let line = error.location().map(|location| location.line());
            Problem::manifest("", yaml_error_message(&error)).at_line(line)
        })
    }

    /// The variable the compose file publishes a port with.
    pub fn port_variable(id: &str) -> String {
        format!("PORT_{}", id.to_ascii_uppercase().replace('-', "_"))
    }

    /// The variable the compose file mounts a share with.
    pub fn share_variable(id: &str) -> String {
        format!("SHARE_{}", id.to_ascii_uppercase().replace('-', "_"))
    }

    pub fn port(&self, id: &str) -> Option<&Port> {
        self.ports.iter().find(|port| port.id == id)
    }

    /// Checks what the manifest says on its own; `folder` is the name of the
    /// app folder it was read from.
    pub fn check(&self, folder: &str) -> Vec<Problem> {
        let mut problems = Vec::new();
        let mut report = |at: &str, message: String| problems.push(Problem::manifest(at, message));

        if !is_slug(&self.id, 40) {
            report("id", "must be 2 to 40 lower-case letters, digits and single hyphens".into());
        } else if self.id != folder {
            report("id", format!("must equal the folder name `{folder}`"));
        }
        text(&mut report, "name", &self.name, 40, true);
        if let Some(version) = &self.version
            && (version.is_empty() || version.len() > 40 || version.contains(char::is_whitespace))
        {
            report("version", "must be 1 to 40 characters without spaces".into());
        }
        text(&mut report, "tagline", &self.tagline, 80, true);
        text(&mut report, "description", &self.description, 3000, false);
        if self.categories.is_empty() || self.categories.len() > 3 {
            report("categories", "name one to three categories".into());
        } else if has_duplicates(self.categories.iter()) {
            report("categories", "lists a category twice".into());
        }
        text(&mut report, "developer", &self.developer, 80, true);
        url(&mut report, "website", &self.website);
        url(&mut report, "source", &self.source);
        if self.license.is_empty()
            || self.license.len() > 80
            || !self.license.chars().all(|c| c.is_ascii_alphanumeric() || " .+-()".contains(c))
        {
            report("license", "must be an SPDX licence expression, such as `GPL-3.0-only`".into());
        }
        if self.maintainers.is_empty() {
            report("maintainers", "name at least one GitHub account".into());
        }
        for (i, maintainer) in self.maintainers.iter().enumerate() {
            if !is_github_login(maintainer) {
                report(&format!("maintainers[{i}]"), format!("`{maintainer}` is not a GitHub account name"));
            }
        }
        if !self.architectures.contains(&Architecture::Amd64) {
            report("architectures", "must include amd64, the architecture zendiloOS runs on".into());
        } else if has_duplicates(self.architectures.iter()) {
            report("architectures", "lists an architecture twice".into());
        }
        if let Some(notes) = &self.notes {
            text(&mut report, "notes", notes, 2000, false);
        }

        self.check_ports(&mut report);
        self.check_shares(&mut report);
        self.check_settings(&mut report);
        self.check_permissions(&mut report);
        self.check_translations(&mut report);
        problems
    }

    fn check_ports(&self, report: &mut impl FnMut(&str, String)) {
        let mut ids = BTreeSet::new();
        for (i, port) in self.ports.iter().enumerate() {
            let at = format!("ports[{i}]");
            if !is_slug(&port.id, 30) {
                report(&format!("{at}.id"), "must be lower-case letters, digits and single hyphens".into());
            } else if !ids.insert(port.id.as_str()) {
                report(&format!("{at}.id"), format!("`{}` is declared twice", port.id));
            }
            text(report, &format!("{at}.label"), &port.label, 60, true);
            if port.default == 0 {
                report(&format!("{at}.default"), "must be a port from 1 to 65535".into());
            } else if RESERVED_HOST_PORTS.contains(&port.default) {
                report(&format!("{at}.default"), format!("{} is used by zendiloOS itself", port.default));
            }
            if port.fixed && !self.permissions.host_network {
                report(
                    &format!("{at}.fixed"),
                    "only an app with `permissions.host_network` has fixed ports".into(),
                );
            }
        }
        if let Some(web) = &self.web {
            if self.port(&web.port).is_none() {
                report("web.port", format!("`{}` is not declared under ports", web.port));
            }
            if !web.path.starts_with('/') || web.path.contains(char::is_whitespace) {
                report("web.path", "must start with `/` and contain no spaces".into());
            }
        }
    }

    fn check_shares(&self, report: &mut impl FnMut(&str, String)) {
        let mut ids = BTreeSet::new();
        for (i, share) in self.shares.iter().enumerate() {
            let at = format!("shares[{i}]");
            if !is_slug(&share.id, 30) {
                report(&format!("{at}.id"), "must be lower-case letters, digits and single hyphens".into());
            } else if !ids.insert(share.id.as_str()) {
                report(&format!("{at}.id"), format!("`{}` is declared twice", share.id));
            }
            text(report, &format!("{at}.label"), &share.label, 60, true);
            if let Some(description) = &share.description {
                text(report, &format!("{at}.description"), description, 300, false);
            }
        }
    }

    fn check_settings(&self, report: &mut impl FnMut(&str, String)) {
        let mut ids = BTreeSet::new();
        for (i, setting) in self.settings.iter().enumerate() {
            let at = format!("settings[{i}]");
            let id = setting.id.as_str();
            if !is_variable_name(id) {
                report(&format!("{at}.id"), "must be upper-case letters, digits and underscores".into());
            } else if BUILT_IN_VARIABLES.contains(&id) || id.starts_with("PORT_") || id.starts_with("SHARE_")
            {
                report(&format!("{at}.id"), format!("`{id}` is reserved for a variable zendiloOS provides"));
            } else if !ids.insert(id) {
                report(&format!("{at}.id"), format!("`{id}` is declared twice"));
            }
            text(report, &format!("{at}.label"), &setting.label, 60, true);
            if let Some(description) = &setting.description {
                text(report, &format!("{at}.description"), description, 300, false);
            }
            if !setting.generate && (setting.length.is_some() || setting.alphabet.is_some()) {
                report(
                    &format!("{at}.generate"),
                    "`length` and `alphabet` describe a generated value".into(),
                );
            }
            if setting.length.is_some_and(|length| !(16..=128).contains(&length)) {
                report(&format!("{at}.length"), "must be 16 to 128 characters".into());
            }
            if setting.generate {
                if setting.kind != SettingKind::Password {
                    report(&format!("{at}.generate"), "only a password can be generated".into());
                }
                if setting.default.is_some() || setting.required {
                    report(
                        &format!("{at}.generate"),
                        "a generated value has no default and is not asked for".into(),
                    );
                }
            }
            if let Some(default) = &setting.default {
                let fits = match setting.kind {
                    SettingKind::Text => default.is_string(),
                    SettingKind::Password => false,
                    SettingKind::Number => default.is_number(),
                    SettingKind::Boolean => default.is_boolean(),
                };
                if !fits {
                    let message = match setting.kind {
                        SettingKind::Password => "a password has no default; use `generate: true`",
                        _ => "does not match the setting's type",
                    };
                    report(&format!("{at}.default"), message.into());
                }
            }
        }
    }

    fn check_permissions(&self, report: &mut impl FnMut(&str, String)) {
        let permissions = &self.permissions;
        for (i, device) in permissions.devices.iter().enumerate() {
            if device_kind(device).is_none() {
                report(&format!("permissions.devices[{i}]"), device_refusal(device));
            }
        }
        if has_duplicates(permissions.devices.iter()) {
            report("permissions.devices", "lists a device twice".into());
        }
        for (i, capability) in permissions.capabilities.iter().enumerate() {
            if !GRANTABLE_CAPABILITIES.contains(&capability.as_str()) {
                report(
                    &format!("permissions.capabilities[{i}]"),
                    format!(
                        "`{capability}` cannot be granted; possible are {} (written without `CAP_`)",
                        GRANTABLE_CAPABILITIES.join(", ")
                    ),
                );
            }
        }
        if has_duplicates(permissions.capabilities.iter()) {
            report("permissions.capabilities", "lists a capability twice".into());
        }
    }

    fn check_translations(&self, report: &mut impl FnMut(&str, String)) {
        for (language, translation) in &self.translations {
            let at = format!("translations.{language}");
            if language.len() != 2 || !language.chars().all(|c| c.is_ascii_lowercase()) || language == "en" {
                report(&at, "use a two-letter ISO 639-1 code other than `en`, such as `de`".into());
            }
            if *translation == Translation::default() {
                report(&at, "translates nothing".into());
            }
            if let Some(name) = &translation.name {
                text(report, &format!("{at}.name"), name, 40, true);
            }
            if let Some(tagline) = &translation.tagline {
                text(report, &format!("{at}.tagline"), tagline, 80, true);
            }
            if let Some(description) = &translation.description {
                text(report, &format!("{at}.description"), description, 3000, false);
            }
            if let Some(notes) = &translation.notes {
                if self.notes.is_none() {
                    report(&format!("{at}.notes"), "translates notes the app does not have".into());
                }
                text(report, &format!("{at}.notes"), notes, 2000, false);
            }
        }
    }
}

/// The first line of a YAML error, without the `error: line 3 column 1:`
/// the problem shows on its own.
pub(crate) fn yaml_error_message(error: &serde_saphyr::Error) -> String {
    let message = error.to_string();
    let mut message = message.lines().next().unwrap_or_default().trim();
    message = message.strip_prefix("error:").unwrap_or(message).trim_start();
    if let Some(rest) = message.strip_prefix("line ")
        && let Some((_, rest)) = rest.split_once(": ")
    {
        message = rest;
    }
    message.to_owned()
}

/// Why a device cannot be handed to an app.
pub(crate) fn device_refusal(device: &str) -> String {
    let kinds: Vec<&str> = DEVICE_KINDS.iter().map(|(kind, _)| *kind).collect();
    format!("`{device}` is not available to apps; possible are devices below {}", kinds.join(", "))
}

fn text(report: &mut impl FnMut(&str, String), at: &str, value: &str, max: usize, one_line: bool) {
    if value.trim().is_empty() {
        report(at, "must not be empty".into());
    } else if value.chars().count() > max {
        report(at, format!("must be at most {max} characters"));
    } else if one_line && value.contains('\n') {
        report(at, "must be a single line".into());
    }
}

fn url(report: &mut impl FnMut(&str, String), at: &str, value: &str) {
    let valid = value
        .strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty() && rest.len() < 300 && !rest.contains(char::is_whitespace));
    if !valid {
        report(at, "must be an https:// address".into());
    }
}

/// Lower-case letters and digits, joined by single hyphens.
pub(crate) fn is_slug(value: &str, max: usize) -> bool {
    (2..=max).contains(&value.len())
        && value.split('-').all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

pub(crate) fn is_variable_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && value.len() <= 40
}

fn is_github_login(value: &str) -> bool {
    (1..=39).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn has_duplicates<T: Ord>(items: impl Iterator<Item = T>) -> bool {
    let mut seen = BTreeSet::new();
    items.into_iter().any(|item| !seen.insert(item))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const MINIMAL: &str = "\
id: demo
name: Demo
version: 1.2.3
tagline: Shows what an app needs
description: A longer text about the demo.
categories: [productivity]
developer: Demo developers
website: https://example.org
source: https://github.com/example/demo
license: MIT
maintainers: [mavnezz]
architectures: [amd64, arm64]
";

    pub(crate) fn minimal() -> Manifest {
        Manifest::parse(MINIMAL).unwrap()
    }

    fn messages(manifest: &Manifest) -> Vec<String> {
        manifest.check("demo").iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_minimal_manifest_is_fine() {
        assert_eq!(messages(&minimal()), Vec::<String>::new());
    }

    #[test]
    fn unknown_fields_are_errors_with_a_line() {
        let problem = Manifest::parse(&format!("{MINIMAL}colour: red\n")).unwrap_err();
        assert_eq!(problem.line, Some(13));
        assert!(problem.message.contains("colour"), "{}", problem.message);
    }

    #[test]
    fn the_id_must_match_the_folder() {
        let mut manifest = minimal();
        manifest.id = "other".into();
        assert_eq!(messages(&manifest), ["app.yaml: id: must equal the folder name `demo`"]);
        manifest.id = "Demo_App".into();
        assert!(messages(&manifest)[0].contains("lower-case"));
    }

    #[test]
    fn web_must_point_to_a_declared_port() {
        let mut manifest = minimal();
        manifest.web = Some(Web { port: "web".into(), path: "/".into(), scheme: WebScheme::Http });
        assert_eq!(messages(&manifest), ["app.yaml: web.port: `web` is not declared under ports"]);
    }

    #[test]
    fn ports_must_not_take_what_the_nas_uses() {
        let mut manifest = minimal();
        manifest.ports.push(Port { id: "web".into(), label: "Web".into(), default: 445, fixed: false });
        assert_eq!(messages(&manifest), ["app.yaml: ports[0].default: 445 is used by zendiloOS itself"]);
    }

    #[test]
    fn fixed_ports_need_the_host_network() {
        let mut manifest = minimal();
        manifest.ports.push(Port { id: "web".into(), label: "Web".into(), default: 8123, fixed: true });
        assert!(messages(&manifest)[0].contains("host_network"));
        manifest.permissions.host_network = true;
        assert!(messages(&manifest).is_empty());
    }

    #[test]
    fn settings_must_not_shadow_built_in_variables() {
        let mut manifest = minimal();
        for id in ["PUID", "PORT_WEB", "lower"] {
            manifest.settings = vec![Setting {
                id: id.into(),
                label: "x".into(),
                description: None,
                kind: SettingKind::Text,
                default: None,
                required: false,
                generate: false,
                length: None,
                alphabet: None,
            }];
            assert_eq!(messages(&manifest).len(), 1, "{id}");
        }
    }

    #[test]
    fn only_passwords_are_generated_and_they_have_no_default() {
        let mut manifest = minimal();
        let mut setting = Setting {
            id: "DB_PASSWORD".into(),
            label: "Database password".into(),
            description: None,
            kind: SettingKind::Password,
            default: None,
            required: false,
            generate: true,
            length: None,
            alphabet: None,
        };
        manifest.settings = vec![setting.clone()];
        assert!(messages(&manifest).is_empty());
        setting.kind = SettingKind::Text;
        manifest.settings = vec![setting.clone()];
        assert_eq!(messages(&manifest), ["app.yaml: settings[0].generate: only a password can be generated"]);
        setting.kind = SettingKind::Password;
        setting.generate = false;
        setting.default = Some("secret".into());
        manifest.settings = vec![setting];
        assert!(messages(&manifest)[0].contains("use `generate: true`"));
    }

    #[test]
    fn generated_values_may_be_hex_and_longer() {
        let mut manifest = minimal();
        let mut setting = Setting {
            id: "SECRET_ENCRYPTION_KEY".into(),
            label: "Encryption key".into(),
            description: None,
            kind: SettingKind::Password,
            default: None,
            required: false,
            generate: true,
            length: Some(64),
            alphabet: Some(Alphabet::Hex),
        };
        manifest.settings = vec![setting.clone()];
        assert!(messages(&manifest).is_empty());
        setting.length = Some(8);
        manifest.settings = vec![setting.clone()];
        assert_eq!(messages(&manifest), ["app.yaml: settings[0].length: must be 16 to 128 characters"]);
        setting.generate = false;
        setting.length = None;
        manifest.settings = vec![setting];
        assert_eq!(
            messages(&manifest),
            ["app.yaml: settings[0].generate: `length` and `alphabet` describe a generated value"]
        );
    }

    #[test]
    fn permissions_are_limited() {
        let mut manifest = minimal();
        manifest.permissions.devices = vec!["/dev/sda".into(), "/dev/ttyUSB".into(), "/dev/ttyUSB0".into()];
        manifest.permissions.capabilities = vec!["SYS_ADMIN".into()];
        let messages = messages(&manifest);
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].contains("`/dev/sda` is not available to apps"));
        assert!(messages[1].contains("`/dev/ttyUSB` is not available to apps"));
        assert!(messages[2].contains("`SYS_ADMIN` cannot be granted"));
    }

    #[test]
    fn device_kinds() {
        for (path, kind) in [
            ("/dev/dri", "/dev/dri"),
            ("/dev/dri/renderD128", "/dev/dri"),
            ("/dev/ttyUSB0", "/dev/ttyUSB*"),
            ("/dev/ttyACM12", "/dev/ttyACM*"),
            ("/dev/serial/by-id/usb-ITead_Sonoff_Zigbee-if00-port0", "/dev/serial/by-id"),
            ("/dev/video0", "/dev/video*"),
            ("/dev/apex_0", "/dev/apex_*"),
        ] {
            assert_eq!(device_kind(path), Some(kind), "{path}");
        }
        for path in
            ["/dev/sda", "/dev/nvme0n1", "/dev/mem", "/dev/tty1", "/dev/ttyUSB", "/dev/ttyUSB0x", "/dev/drix"]
        {
            assert_eq!(device_kind(path), None, "{path}");
        }
    }

    #[test]
    fn the_version_is_optional() {
        let mut manifest = minimal();
        manifest.version = None;
        assert!(messages(&manifest).is_empty());
        manifest.version = Some("1 2".into());
        assert_eq!(messages(&manifest), ["app.yaml: version: must be 1 to 40 characters without spaces"]);
    }

    #[test]
    fn translations_use_language_codes() {
        let mut manifest = minimal();
        manifest.translations.insert(
            "german".into(),
            Translation { tagline: Some("Zeigt, was eine App braucht".into()), ..Translation::default() },
        );
        manifest.translations.insert("de".into(), Translation::default());
        let messages = messages(&manifest);
        assert_eq!(messages.len(), 2, "{messages:?}");
    }

    #[test]
    fn slugs() {
        assert!(is_slug("uptime-kuma", 40));
        assert!(is_slug("n8n", 40));
        for bad in ["a", "-a", "a-", "a--b", "A", "a_b"] {
            assert!(!is_slug(bad, 40), "{bad}");
        }
    }
}
