//! The policy, checked on the normalized compose file
//! (`docker compose config --format json`).
//!
//! Service keys are allowed by name: a key Compose learns in a future version
//! is refused until someone has looked at what it does.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use super::{ImageRef, clean_path, is_below};
use crate::manifest::{GRANTABLE_CAPABILITIES, device_kind, device_refusal};
use crate::{Access, Manifest, Placement, Problem};

/// Service keys that need no further check.
const FREE_KEYS: [&str; 35] = [
    "command",
    "cap_drop",
    "cpu_shares",
    "cpus",
    "depends_on",
    "dns",
    "dns_opt",
    "dns_search",
    "domainname",
    "entrypoint",
    "environment",
    "expose",
    "extra_hosts",
    "group_add",
    "healthcheck",
    "hostname",
    "init",
    "links",
    "mac_address",
    "mem_limit",
    "mem_reservation",
    "memswap_limit",
    "pids_limit",
    "platform",
    "pull_policy",
    "read_only",
    "shm_size",
    "stdin_open",
    "stop_grace_period",
    "stop_signal",
    "tmpfs",
    "tty",
    "ulimits",
    "user",
    "working_dir",
];

/// Service keys that are refused, with the reason.
const REFUSED_KEYS: [(&str, &str); 18] = [
    ("privileged", "privileged containers control the whole machine"),
    ("pid", "would share the process namespace with the host"),
    ("uts", "would share the host name namespace with the host"),
    ("userns_mode", "would change the container's user namespace"),
    ("cgroup", "would share the cgroup namespace with the host"),
    ("cgroup_parent", "would move the container out of Docker's resource control"),
    ("device_cgroup_rules", "would open devices beyond those granted"),
    ("gpus", "is not available to apps yet; the `/dev/dri` device covers video acceleration"),
    ("runtime", "zendiloOS runs every app with Docker's default runtime"),
    ("volumes_from", "mount folders below `${APP_DATA}` instead"),
    ("container_name", "zendiloOS names the containers; a fixed name clashes with other apps"),
    ("sysctls", "is not available to apps"),
    ("use_api_socket", "mount `/var/run/docker.sock` with `permissions.docker_socket` instead"),
    ("provider", "runs a program on the NAS itself"),
    ("post_start", "lifecycle hooks are not available to apps"),
    ("pre_stop", "lifecycle hooks are not available to apps"),
    ("external_links", "reaches into containers outside the app"),
    ("oom_kill_disable", "could let one app starve the NAS of memory"),
];

/// Docker's default capabilities: adding one back after `cap_drop: [ALL]`
/// grants nothing beyond what every container has.
const DEFAULT_CAPABILITIES: [&str; 14] = [
    "AUDIT_WRITE",
    "CHOWN",
    "DAC_OVERRIDE",
    "FOWNER",
    "FSETID",
    "KILL",
    "MKNOD",
    "NET_BIND_SERVICE",
    "NET_RAW",
    "SETFCAP",
    "SETGID",
    "SETPCAP",
    "SETUID",
    "SYS_CHROOT",
];

/// Where Debian's Docker listens; `/var/run` is a link to `/run`.
const DOCKER_SOCKETS: [&str; 2] = ["/var/run/docker.sock", "/run/docker.sock"];

const NAMED_VOLUME: &str = "Docker volumes keep data outside the NAS's volumes, \
    without snapshots or backups; mount a folder below `${APP_DATA}` instead";

/// Checks a normalized compose file against the policy, with the values
/// zendiloOS fills in (or [`Placement::stand_in`] when validating).
pub fn check_normalized(normalized: &Value, manifest: &Manifest, placement: &Placement) -> Vec<Problem> {
    let mut check = Check {
        manifest,
        placement,
        problems: Vec::new(),
        binds: Vec::new(),
        published: Vec::new(),
        shares_used: BTreeSet::new(),
        devices_used: BTreeSet::new(),
        capabilities_used: BTreeSet::new(),
        host_network_used: false,
        docker_socket_used: false,
    };
    check.root(normalized);
    check.finish();
    check.problems
}

struct Check<'a> {
    manifest: &'a Manifest,
    placement: &'a Placement,
    problems: Vec<Problem>,
    binds: Vec<Bind>,
    published: Vec<Published>,
    shares_used: BTreeSet<String>,
    devices_used: BTreeSet<String>,
    capabilities_used: BTreeSet<&'static str>,
    host_network_used: bool,
    docker_socket_used: bool,
}

struct Bind {
    source: String,
    at: String,
}

struct Published {
    port: String,
    protocol: String,
}

impl Check<'_> {
    fn error(&mut self, at: impl Into<String>, message: impl Into<String>) {
        self.problems.push(Problem::compose(at, message));
    }

    fn root(&mut self, value: &Value) {
        let Some(root) = value.as_object() else {
            self.error("", "must describe at least one service");
            return;
        };
        match root.get("services").and_then(Value::as_object) {
            Some(services) if !services.is_empty() => {
                for (name, service) in services {
                    self.service(name, service, services);
                }
            }
            _ => self.error("services", "must describe at least one service"),
        }
        for (key, value) in root {
            match key.as_str() {
                "name" | "services" => {}
                "networks" => self.networks(value),
                "volumes" => {
                    for name in value.as_object().into_iter().flat_map(Map::keys) {
                        self.error(format!("volumes.{name}"), NAMED_VOLUME);
                    }
                }
                key if key.starts_with("x-") => {}
                key => self.error(key, "is not available to apps"),
            }
        }
    }

    fn service(&mut self, name: &str, value: &Value, services: &Map<String, Value>) {
        let at = format!("services.{name}");
        let Some(service) = value.as_object() else {
            self.error(at, "must be a mapping");
            return;
        };
        if !service.contains_key("image") {
            self.error(&at, "needs an `image`; zendiloOS does not build images");
        }
        if !service.contains_key("restart") {
            self.error(
                format!("{at}.restart"),
                "set `unless-stopped`, so the app comes back after the NAS restarts (`\"no\"` for a job that runs once)",
            );
        }
        let host_network = service.get("network_mode").and_then(Value::as_str) == Some("host");
        for (key, value) in service {
            let at = format!("{at}.{key}");
            match key.as_str() {
                "image" => self.image(at, value),
                "restart" => self.restart(at, value),
                "ports" if host_network => self.error(
                    at,
                    "a service on the host network listens on its ports directly; declare them in app.yaml with `fixed: true`",
                ),
                "ports" => self.ports(&at, value),
                "volumes" => self.volumes(&at, value),
                "network_mode" => self.network_mode(at, value, services),
                "networks" => self.service_networks(&at, value),
                "cap_add" => self.capabilities(&at, value),
                "devices" => self.devices(&at, value),
                "security_opt" => self.security_options(&at, value),
                "ipc" => self.ipc(at, value, services),
                "logging" => self.logging(&at, value),
                "deploy" => self.deploy(&at, value),
                "labels" => self.labels(&at, value),
                key if FREE_KEYS.contains(&key) || key.starts_with("x-") => {}
                key => {
                    let reason = REFUSED_KEYS.iter().find(|(name, _)| *name == key).map(|(_, reason)| *reason);
                    self.error(at, reason.unwrap_or("is not available to apps"));
                }
            }
        }
    }

    fn image(&mut self, at: String, value: &Value) {
        match value.as_str().map(ImageRef::parse) {
            Some(Ok(_)) => {}
            Some(Err(message)) => self.error(at, message),
            None => self.error(at, "must be an image reference"),
        }
    }

    fn restart(&mut self, at: String, value: &Value) {
        let valid = value.as_str().is_some_and(|policy| {
            matches!(policy, "no" | "always" | "unless-stopped" | "on-failure")
                || policy.strip_prefix("on-failure:").is_some_and(|n| n.parse::<u32>().is_ok())
        });
        if !valid {
            self.error(at, "must be `unless-stopped`, `always`, `on-failure` or `\"no\"`");
        }
    }

    fn ports(&mut self, at: &str, value: &Value) {
        for (i, entry) in value.as_array().into_iter().flatten().enumerate() {
            let at = format!("{at}[{i}]");
            let Some(entry) = entry.as_object() else {
                self.error(at, "must be a port mapping");
                continue;
            };
            for key in entry.keys() {
                match key.as_str() {
                    "mode" | "target" | "published" | "protocol" | "app_protocol" | "name" => {}
                    "host_ip" => self.error(
                        format!("{at}.host_ip"),
                        "zendiloOS decides which addresses a port listens on; leave out the address",
                    ),
                    key => self.error(format!("{at}.{key}"), "is not available to apps"),
                }
            }
            let host_port = entry.get("published").and_then(|published| match published {
                Value::String(text) => text.parse::<u16>().ok(),
                Value::Number(number) => number.as_u64().and_then(|n| u16::try_from(n).ok()),
                _ => None,
            });
            let declared = host_port.and_then(|host_port| {
                let (id, _) = self.placement.ports.iter().find(|(_, port)| **port == host_port)?;
                self.manifest.port(id).filter(|port| !port.fixed)
            });
            match declared {
                Some(port) => {
                    let protocol = entry.get("protocol").and_then(Value::as_str).unwrap_or("tcp").to_owned();
                    self.published.push(Published { port: port.id.clone(), protocol });
                }
                None => self.error(
                    at,
                    "publish host ports as `${PORT_<ID>}:<container port>`, with the port declared under `ports` in app.yaml",
                ),
            }
        }
    }

    fn volumes(&mut self, at: &str, value: &Value) {
        for (i, entry) in value.as_array().into_iter().flatten().enumerate() {
            let at = format!("{at}[{i}]");
            let Some(entry) = entry.as_object() else {
                self.error(at, "must be a mount");
                continue;
            };
            match entry.get("type").and_then(Value::as_str) {
                Some("bind") => self.bind(at, entry),
                Some("tmpfs") => {}
                Some("volume") => self.error(at, NAMED_VOLUME),
                Some(kind) => self.error(at, format!("`{kind}` mounts are not available to apps")),
                None => self.error(at, "must name a mount type"),
            }
        }
    }

    fn bind(&mut self, at: String, entry: &Map<String, Value>) {
        for (key, value) in entry {
            match key.as_str() {
                "type" | "source" | "target" | "read_only" | "consistency" => {}
                "bind" => self.bind_options(&at, value),
                key => self.error(format!("{at}.{key}"), "is not available to apps"),
            }
        }
        let read_only = entry.get("read_only").and_then(Value::as_bool).unwrap_or(false);
        let Some(source) = entry.get("source").and_then(Value::as_str).and_then(clean_path) else {
            self.error(format!("{at}.source"), "must be an absolute path without `..`");
            return;
        };
        if is_below(&source, &self.placement.app_data) {
            self.binds.push(Bind { source, at });
            return;
        }
        if DOCKER_SOCKETS.contains(&source.as_str()) {
            self.docker_socket_used = true;
            if !self.manifest.permissions.docker_socket {
                self.error(
                    at,
                    "the Docker socket needs `permissions.docker_socket: true` in app.yaml, which the administrator \
                     confirms: whoever holds it controls every container and the NAS itself",
                );
            }
            return;
        }
        for (id, path) in &self.placement.shares {
            if source == *path {
                let declared = self.manifest.shares.iter().find(|share| share.id == *id);
                if declared.is_some_and(|share| share.access == Access::ReadOnly) && !read_only {
                    self.problems.push(Problem::compose(
                        &at,
                        format!("the share `{id}` is declared read-only in app.yaml; mount it with `:ro`"),
                    ));
                }
                self.shares_used.insert(id.clone());
                self.binds.push(Bind { source, at });
                return;
            }
            if is_below(&source, path) {
                self.error(
                    format!("{at}.source"),
                    "mount a share as a whole; what is inside a share can be changed by others",
                );
                return;
            }
        }
        let message = if matches!(source.as_str(), "/etc/localtime" | "/etc/timezone") {
            "the time zone comes from `TZ: ${TZ}` under `environment`"
        } else {
            "bind mounts may only come from `${APP_DATA}` or a share declared in app.yaml (`${SHARE_<ID>}`)"
        };
        self.error(format!("{at}.source"), message);
    }

    fn bind_options(&mut self, at: &str, value: &Value) {
        for (key, value) in value.as_object().into_iter().flatten() {
            let allowed = match key.as_str() {
                "create_host_path" => true,
                "propagation" => matches!(value.as_str(), Some("rprivate" | "private")),
                _ => false,
            };
            if !allowed {
                self.error(format!("{at}.bind.{key}"), "is not available to apps");
            }
        }
    }

    fn network_mode(&mut self, at: String, value: &Value, services: &Map<String, Value>) {
        match value.as_str() {
            Some("none") => {}
            Some("host") => {
                self.host_network_used = true;
                if !self.manifest.permissions.host_network {
                    self.error(
                        at,
                        "the host network needs `permissions.host_network: true` in app.yaml, which the administrator confirms",
                    );
                }
            }
            Some(mode) if mode.strip_prefix("service:").is_some_and(|name| services.contains_key(name)) => {}
            _ => self.error(
                at,
                "may be `host` (with permission), `none` or `service:<name>`; leave it out to use the app's own network",
            ),
        }
    }

    fn service_networks(&mut self, at: &str, value: &Value) {
        for (network, options) in value.as_object().into_iter().flatten() {
            for key in options.as_object().into_iter().flat_map(Map::keys) {
                if !matches!(
                    key.as_str(),
                    "aliases" | "ipv4_address" | "ipv6_address" | "priority" | "gw_priority" | "mac_address"
                ) {
                    self.error(format!("{at}.{network}.{key}"), "is not available to apps");
                }
            }
        }
    }

    fn networks(&mut self, value: &Value) {
        for (key, network) in value.as_object().into_iter().flatten() {
            let at = format!("networks.{key}");
            for (option, value) in network.as_object().into_iter().flatten() {
                let at = format!("{at}.{option}");
                match option.as_str() {
                    "name" => {
                        if value.as_str() != Some(&format!("{}_{key}", self.placement.project)) {
                            self.error(at, "leave out `name`; zendiloOS names the networks of each app");
                        }
                    }
                    "ipam" => {
                        if value.as_object().is_some_and(|ipam| !ipam.is_empty()) {
                            self.error(
                                at,
                                "fixed address ranges may clash with the home network; leave out `ipam`",
                            );
                        }
                    }
                    "driver" => {
                        if value.as_str() != Some("bridge") {
                            self.error(at, "only `bridge` networks are available to apps");
                        }
                    }
                    "external" => self.error(at, "would join a network outside the app"),
                    "internal" | "enable_ipv6" | "labels" | "attachable" => {}
                    option if option.starts_with("x-") => {}
                    _ => self.error(at, "is not available to apps"),
                }
            }
        }
    }

    fn capabilities(&mut self, at: &str, value: &Value) {
        for (i, capability) in value.as_array().into_iter().flatten().enumerate() {
            let at = format!("{at}[{i}]");
            let name = capability.as_str().unwrap_or_default().to_ascii_uppercase();
            let name = name.strip_prefix("CAP_").unwrap_or(&name);
            if DEFAULT_CAPABILITIES.contains(&name) {
                continue;
            }
            match GRANTABLE_CAPABILITIES.iter().find(|grantable| **grantable == name) {
                Some(grantable) => {
                    self.capabilities_used.insert(grantable);
                    if !self.manifest.permissions.capabilities.iter().any(|declared| declared == grantable) {
                        self.error(
                            at,
                            format!(
                                "`{name}` needs `permissions.capabilities: [{name}]` in app.yaml, which the administrator confirms"
                            ),
                        );
                    }
                }
                None => self.error(at, format!("`{name}` is not available to apps")),
            }
        }
    }

    fn devices(&mut self, at: &str, value: &Value) {
        for (i, device) in value.as_array().into_iter().flatten().enumerate() {
            let at = format!("{at}[{i}]");
            let source =
                device.get("source").and_then(Value::as_str).and_then(clean_path).unwrap_or_default();
            if device_kind(&source).is_none() {
                self.error(at, device_refusal(&source));
                continue;
            }
            let declared =
                self.manifest.permissions.devices.iter().find(|declared| is_below(&source, declared));
            match declared {
                Some(declared) => {
                    self.devices_used.insert(declared.clone());
                }
                None => self.error(
                    at,
                    format!(
                        "`{source}` needs `permissions.devices: [{source}]` in app.yaml, which the administrator confirms"
                    ),
                ),
            }
        }
    }

    fn security_options(&mut self, at: &str, value: &Value) {
        for (i, option) in value.as_array().into_iter().flatten().enumerate() {
            let allowed = matches!(
                option.as_str(),
                Some("no-new-privileges" | "no-new-privileges:true" | "no-new-privileges=true")
            );
            if !allowed {
                self.error(format!("{at}[{i}]"), "only `no-new-privileges:true` is available to apps");
            }
        }
    }

    fn ipc(&mut self, at: String, value: &Value, services: &Map<String, Value>) {
        let allowed = value.as_str().is_some_and(|mode| {
            matches!(mode, "private" | "shareable")
                || mode.strip_prefix("service:").is_some_and(|name| services.contains_key(name))
        });
        if !allowed {
            self.error(at, "may be `private`, `shareable` or `service:<name>` of this app");
        }
    }

    fn logging(&mut self, at: &str, value: &Value) {
        for (key, value) in value.as_object().into_iter().flatten() {
            match key.as_str() {
                "options" => {}
                "driver" if matches!(value.as_str(), Some("json-file" | "local")) => {}
                _ => self.error(
                    format!("{at}.{key}"),
                    "zendiloOS reads the logs; only `json-file` and `local` work",
                ),
            }
        }
    }

    fn deploy(&mut self, at: &str, value: &Value) {
        for (key, value) in value.as_object().into_iter().flatten() {
            let at = format!("{at}.{key}");
            match key.as_str() {
                "placement" if value.as_object().is_some_and(Map::is_empty) => {}
                "resources" => {
                    for (kind, limits) in value.as_object().into_iter().flatten() {
                        if !matches!(kind.as_str(), "limits" | "reservations") {
                            self.error(format!("{at}.{kind}"), "is not available to apps");
                            continue;
                        }
                        for key in limits.as_object().into_iter().flat_map(Map::keys) {
                            if !matches!(key.as_str(), "cpus" | "memory" | "pids") {
                                self.error(format!("{at}.{kind}.{key}"), "is not available to apps");
                            }
                        }
                    }
                }
                _ => self.error(at, "only `resources` (cpus, memory, pids) is available to apps"),
            }
        }
    }

    fn labels(&mut self, at: &str, value: &Value) {
        for key in value.as_object().into_iter().flat_map(Map::keys) {
            if key.starts_with("com.docker.") || key.starts_with("zendiloos.") {
                self.error(format!("{at}.{key}"), "this label prefix is reserved");
            }
        }
    }

    /// Checks that need every service: ports, shares, permissions, mounts
    /// inside mounts.
    fn finish(&mut self) {
        // A port is published at least once, and at most once per protocol:
        // a BitTorrent client listens on one number for TCP and UDP.
        for (i, port) in self.manifest.ports.iter().enumerate() {
            let protocols: Vec<&str> =
                self.published.iter().filter(|p| p.port == port.id).map(|p| p.protocol.as_str()).collect();
            let at = format!("ports[{i}]");
            if port.fixed {
                continue;
            }
            if protocols.is_empty() {
                let variable = Manifest::port_variable(&port.id);
                self.problems.push(Problem::manifest(
                    at,
                    format!("is not published in compose.yaml as `${{{variable}}}`"),
                ));
            } else if let Some(twice) =
                protocols.iter().enumerate().find(|(j, protocol)| protocols[..*j].contains(protocol))
            {
                let message = format!("is published more than once for {}", twice.1.to_ascii_uppercase());
                self.problems.push(Problem::manifest(at, message));
            }
        }
        if let Some(web) = &self.manifest.web
            && self.published.iter().any(|p| p.port == web.port)
            && !self.published.iter().any(|p| p.port == web.port && p.protocol == "tcp")
        {
            self.problems.push(Problem::manifest("web.port", "the web interface needs a TCP port"));
        }
        for (i, share) in self.manifest.shares.iter().enumerate() {
            if !self.shares_used.contains(&share.id) {
                self.problems.push(Problem::manifest(
                    format!("shares[{i}]"),
                    format!(
                        "is not mounted in compose.yaml as `${{{}}}`",
                        Manifest::share_variable(&share.id)
                    ),
                ));
            }
        }
        let permissions = &self.manifest.permissions;
        for (i, device) in permissions.devices.iter().enumerate() {
            if !self.devices_used.contains(device.as_str()) {
                self.problems.push(Problem::manifest(
                    format!("permissions.devices[{i}]"),
                    "is not used in compose.yaml",
                ));
            }
        }
        for (i, capability) in permissions.capabilities.iter().enumerate() {
            if !self.capabilities_used.contains(capability.as_str()) {
                self.problems.push(Problem::manifest(
                    format!("permissions.capabilities[{i}]"),
                    "is not used in compose.yaml",
                ));
            }
        }
        if permissions.docker_socket && !self.docker_socket_used {
            self.problems.push(Problem::manifest(
                "permissions.docker_socket",
                "no service in compose.yaml mounts `/var/run/docker.sock`",
            ));
        }
        if permissions.host_network && !self.host_network_used {
            self.problems.push(Problem::manifest(
                "permissions.host_network",
                "no service in compose.yaml uses `network_mode: host`",
            ));
        }

        // A container that can write to a mounted folder could put a link
        // where another mount's source lies inside it; Docker would follow
        // the link the next time it starts the app.
        let mut nested = Vec::new();
        for inner in &self.binds {
            if let Some(outer) = self
                .binds
                .iter()
                .find(|outer| outer.source != inner.source && is_below(&inner.source, &outer.source))
            {
                nested.push(Problem::compose(
                    &inner.at,
                    format!(
                        "lies inside the folder mounted at `{}`; a container could replace it with a link to anywhere on the NAS",
                        outer.at
                    ),
                ));
            }
        }
        self.problems.extend(nested);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::manifest::tests::minimal;
    use crate::{Port, Share};

    const IMAGE: &str =
        "example/app:1.2.3@sha256:fea863d0cd9ec445985d9c75fc6ed64e38a1d2766cf0c250a33d553f4fc1b771";

    fn manifest() -> Manifest {
        let mut manifest = minimal();
        manifest.ports = vec![Port { id: "web".into(), label: "Web".into(), default: 8080, fixed: false }];
        manifest.shares = vec![Share {
            id: "media".into(),
            label: "Media".into(),
            description: None,
            access: Access::ReadOnly,
        }];
        manifest
    }

    /// A normalized compose file as Compose writes it, for the stand-ins of
    /// [`manifest`]; `change` adjusts the one service `app`.
    fn compose(change: impl FnOnce(&mut Map<String, Value>)) -> Value {
        let mut service = json!({
            "image": IMAGE,
            "restart": "unless-stopped",
            "command": null,
            "entrypoint": null,
            "networks": { "default": null },
            "ports": [{ "mode": "ingress", "target": 80, "published": "61001", "protocol": "tcp" }],
            "volumes": [
                { "type": "bind", "source": "/zendiloos-stand-in/app-data/config", "target": "/config", "bind": {} },
                {
                    "type": "bind", "source": "/zendiloos-stand-in/share-media", "target": "/media",
                    "read_only": true, "bind": {}
                }
            ]
        });
        change(service.as_object_mut().unwrap());
        json!({
            "name": "demo",
            "networks": { "default": { "name": "demo_default", "ipam": {} } },
            "services": { "app": service }
        })
    }

    fn check_with(manifest: &Manifest, normalized: &Value) -> Vec<String> {
        check_normalized(normalized, manifest, &Placement::stand_in(manifest))
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn check(change: impl FnOnce(&mut Map<String, Value>)) -> Vec<String> {
        check_with(&manifest(), &compose(change))
    }

    #[test]
    fn a_well_behaved_app_passes() {
        assert_eq!(check(|_| {}), Vec::<String>::new());
    }

    #[test]
    fn dangerous_keys_are_refused() {
        let problems = check(|service| {
            service.insert("privileged".into(), json!(true));
            service.insert("pid".into(), json!("host"));
            service.insert("use_api_socket".into(), json!(true));
            service.insert("something_new".into(), json!(1));
        });
        assert_eq!(
            problems,
            [
                "compose.yaml: services.app.pid: would share the process namespace with the host",
                "compose.yaml: services.app.privileged: privileged containers control the whole machine",
                "compose.yaml: services.app.something_new: is not available to apps",
                "compose.yaml: services.app.use_api_socket: mount `/var/run/docker.sock` with `permissions.docker_socket` instead",
            ]
        );
    }

    #[test]
    fn any_tag_is_fine_and_services_restart() {
        let problems = check(|service| {
            service.insert("image".into(), json!("example/app:latest"));
            service.remove("restart");
        });
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("compose.yaml: services.app.restart: set `unless-stopped`"));

        let problems = check(|service| {
            service.remove("image");
        });
        assert_eq!(
            problems,
            ["compose.yaml: services.app: needs an `image`; zendiloOS does not build images"]
        );
    }

    #[test]
    fn bind_mounts_stay_in_app_data_and_shares() {
        let problems = check(|service| {
            service.insert(
                "volumes".into(),
                json!([
                    { "type": "bind", "source": "/etc", "target": "/etc" },
                    { "type": "bind", "source": "/zendiloos-stand-in/app-data/../../etc", "target": "/x" },
                    { "type": "bind", "source": "/zendiloos-stand-in/share-media", "target": "/media" },
                    { "type": "bind", "source": "/zendiloos-stand-in/share-media/movies", "target": "/movies" },
                    { "type": "volume", "source": "data", "target": "/data" }
                ]),
            );
        });
        assert_eq!(
            problems,
            [
                "compose.yaml: services.app.volumes[0].source: bind mounts may only come from `${APP_DATA}` or a share declared in app.yaml (`${SHARE_<ID>}`)",
                "compose.yaml: services.app.volumes[1].source: must be an absolute path without `..`",
                "compose.yaml: services.app.volumes[2]: the share `media` is declared read-only in app.yaml; mount it with `:ro`",
                "compose.yaml: services.app.volumes[3].source: mount a share as a whole; what is inside a share can be changed by others",
                format!("compose.yaml: services.app.volumes[4]: {NAMED_VOLUME}").as_str(),
            ]
        );
    }

    #[test]
    fn the_docker_socket_needs_permission() {
        let socket = |service: &mut Map<String, Value>| {
            service["volumes"].as_array_mut().unwrap().push(
                json!({ "type": "bind", "source": "/var/run/docker.sock", "target": "/var/run/docker.sock" }),
            );
        };
        let problems = check(socket);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("needs `permissions.docker_socket: true`"), "{problems:?}");

        let mut manifest = manifest();
        manifest.permissions.docker_socket = true;
        assert_eq!(check_with(&manifest, &compose(socket)), Vec::<String>::new());
        assert_eq!(
            check_with(&manifest, &compose(|_| {})),
            ["app.yaml: permissions.docker_socket: no service in compose.yaml mounts `/var/run/docker.sock`"]
        );
    }

    #[test]
    fn mounts_inside_mounts_are_refused() {
        let problems = check(|service| {
            let volumes = service["volumes"].as_array_mut().unwrap();
            volumes
                .push(json!({ "type": "bind", "source": "/zendiloos-stand-in/app-data", "target": "/data" }));
        });
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with(
            "compose.yaml: services.app.volumes[0]: lies inside the folder mounted at `services.app.volumes[2]`"
        ));
    }

    #[test]
    fn ports_come_from_the_manifest() {
        let problems = check(|service| {
            service.insert(
                "ports".into(),
                json!([
                    { "mode": "ingress", "target": 80, "published": "8080", "protocol": "tcp" },
                    { "mode": "ingress", "target": 81, "host_ip": "0.0.0.0", "published": "61001", "protocol": "tcp" }
                ]),
            );
        });
        assert_eq!(
            problems,
            [
                "compose.yaml: services.app.ports[0]: publish host ports as `${PORT_<ID>}:<container port>`, with the port declared under `ports` in app.yaml",
                "compose.yaml: services.app.ports[1].host_ip: zendiloOS decides which addresses a port listens on; leave out the address",
            ]
        );
    }

    #[test]
    fn one_port_may_serve_tcp_and_udp_once_each() {
        let ports = |entries: Value| {
            check(|service| {
                service.insert("ports".into(), entries);
            })
        };
        let tcp = json!({ "mode": "ingress", "target": 6881, "published": "61001", "protocol": "tcp" });
        let udp = json!({ "mode": "ingress", "target": 6881, "published": "61001", "protocol": "udp" });
        assert_eq!(ports(json!([tcp.clone(), udp.clone()])), Vec::<String>::new());
        assert_eq!(
            ports(json!([tcp.clone(), udp, tcp])),
            ["app.yaml: ports[0]: is published more than once for TCP"]
        );
    }

    #[test]
    fn the_web_interface_needs_tcp() {
        let mut manifest = manifest();
        manifest.web =
            Some(crate::Web { port: "web".into(), path: "/".into(), scheme: crate::WebScheme::Http });
        let normalized = compose(|service| {
            service["ports"][0]["protocol"] = json!("udp");
        });
        assert_eq!(
            check_with(&manifest, &normalized),
            ["app.yaml: web.port: the web interface needs a TCP port"]
        );
    }

    #[test]
    fn declared_ports_and_shares_must_be_used() {
        let problems = check(|service| {
            service.remove("ports");
            service["volumes"].as_array_mut().unwrap().pop();
        });
        assert_eq!(
            problems,
            [
                "app.yaml: ports[0]: is not published in compose.yaml as `${PORT_WEB}`",
                "app.yaml: shares[0]: is not mounted in compose.yaml as `${SHARE_MEDIA}`",
            ]
        );
    }

    #[test]
    fn the_host_network_needs_permission_and_fixed_ports() {
        let host = |service: &mut Map<String, Value>| {
            service.remove("networks");
            service.insert("network_mode".into(), json!("host"));
        };
        let problems = check(host);
        assert!(
            problems.iter().any(|p| p.contains("needs `permissions.host_network: true`")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("declare them in app.yaml with `fixed: true`")),
            "{problems:?}"
        );

        let mut manifest = manifest();
        manifest.permissions.host_network = true;
        manifest.ports[0].fixed = true;
        manifest.ports[0].default = 8123;
        let normalized = compose(|service| {
            host(service);
            service.remove("ports");
        });
        assert_eq!(check_with(&manifest, &normalized), Vec::<String>::new());
    }

    #[test]
    fn granted_devices_and_capabilities_must_be_declared() {
        let normalized = compose(|service| {
            service.insert(
                "devices".into(),
                json!([
                    { "source": "/dev/dri/renderD128", "target": "/dev/dri/renderD128", "permissions": "rwm" },
                    { "source": "/dev/ttyUSB0", "target": "/dev/ttyUSB0", "permissions": "rwm" },
                    { "source": "/dev/sda", "target": "/dev/sda", "permissions": "rwm" },
                    { "source": "/dev/mem", "target": "/dev/mem", "permissions": "rwm" }
                ]),
            );
            service.insert("cap_drop".into(), json!(["ALL"]));
            service.insert("cap_add".into(), json!(["CHOWN", "CAP_NET_ADMIN", "SYS_ADMIN"]));
        });
        let problems = check_with(&manifest(), &normalized);
        assert_eq!(problems.len(), 6, "{problems:?}");
        assert!(problems[0].contains("`NET_ADMIN` needs `permissions.capabilities: [NET_ADMIN]`"));
        assert!(problems[1].contains("`SYS_ADMIN` is not available"));
        assert!(
            problems[2].contains("`/dev/dri/renderD128` needs `permissions.devices: [/dev/dri/renderD128]`")
        );
        assert!(problems[3].contains("`/dev/ttyUSB0` needs `permissions.devices: [/dev/ttyUSB0]`"));
        assert!(problems[4].contains("`/dev/sda` is not available"));
        assert!(problems[5].contains("`/dev/mem` is not available"));

        let mut manifest = manifest();
        manifest.permissions.devices = vec!["/dev/dri".into(), "/dev/ttyUSB0".into()];
        manifest.permissions.capabilities = vec!["NET_ADMIN".into()];
        assert_eq!(check_with(&manifest, &normalized).len(), 3);
    }

    #[test]
    fn unused_permissions_are_reported() {
        let mut manifest = manifest();
        manifest.permissions.devices = vec!["/dev/dri".into()];
        manifest.permissions.host_network = true;
        let problems = check_with(&manifest, &compose(|_| {}));
        assert_eq!(
            problems,
            [
                "app.yaml: permissions.devices[0]: is not used in compose.yaml",
                "app.yaml: permissions.host_network: no service in compose.yaml uses `network_mode: host`",
            ]
        );
    }

    #[test]
    fn networks_stay_inside_the_app() {
        let mut normalized = compose(|_| {});
        normalized["networks"] = json!({
            "default": { "name": "demo_default", "ipam": {} },
            "lan": { "name": "host", "driver": "macvlan", "external": true }
        });
        normalized["volumes"] = json!({ "data": { "name": "demo_data" } });
        let problems = check_with(&manifest(), &normalized);
        assert_eq!(
            problems,
            [
                "compose.yaml: networks.lan.driver: only `bridge` networks are available to apps",
                "compose.yaml: networks.lan.external: would join a network outside the app",
                "compose.yaml: networks.lan.name: leave out `name`; zendiloOS names the networks of each app",
                format!("compose.yaml: volumes.data: {NAMED_VOLUME}").as_str(),
            ]
        );
    }

    #[test]
    fn resources_and_logging_are_limited() {
        let problems = check(|service| {
            service.insert(
                "deploy".into(),
                json!({ "resources": { "reservations": { "memory": "256M", "devices": [] } }, "placement": {} }),
            );
            service.insert("logging".into(), json!({ "driver": "syslog" }));
            service.insert("security_opt".into(), json!(["no-new-privileges:true", "seccomp:unconfined"]));
        });
        assert_eq!(
            problems,
            [
                "compose.yaml: services.app.deploy.resources.reservations.devices: is not available to apps",
                "compose.yaml: services.app.logging.driver: zendiloOS reads the logs; only `json-file` and `local` work",
                "compose.yaml: services.app.security_opt[1]: only `no-new-privileges:true` is available to apps",
            ]
        );
    }
}
