use std::collections::BTreeMap;

use crate::Manifest;

/// The values zendiloOS fills in when it installs an app: the project name,
/// where the app's data lives, which shares and host ports it got, the
/// account it runs as and the settings.
///
/// For checking an app without installing it, [`Placement::stand_in`] makes
/// up values that are easy to recognise: after Docker has interpolated the
/// compose file, [`crate::compose::check_variable_use`] can tell where each
/// variable ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub project: String,
    pub app_data: String,
    pub puid: String,
    pub pgid: String,
    pub tz: String,
    /// Share id → absolute path of the share the administrator picked.
    pub shares: BTreeMap<String, String>,
    /// Port id → host port.
    pub ports: BTreeMap<String, u16>,
    /// Setting id → value.
    pub settings: BTreeMap<String, String>,
}

/// Every stand-in value contains this, so the compose file must not.
pub(crate) const STAND_IN_MARK: &str = "zendiloos-stand-in";
pub(crate) const STAND_IN_ROOT: &str = "/zendiloos-stand-in/";
pub(crate) const STAND_IN_SETTING: &str = "zendiloos-stand-in-setting-";
pub(crate) const STAND_IN_PUID: &str = "zendiloos-stand-in-puid";
pub(crate) const STAND_IN_PGID: &str = "zendiloos-stand-in-pgid";
pub(crate) const STAND_IN_TZ: &str = "zendiloos-stand-in-tz";
/// Stand-in host ports count up from here; published ports must be numbers.
pub(crate) const STAND_IN_FIRST_PORT: u16 = 61001;

impl Placement {
    /// Stand-in values for checking an app without installing it. Fixed
    /// ports keep their real number, because the app listens on it itself.
    pub fn stand_in(manifest: &Manifest) -> Self {
        let mut next_port = STAND_IN_FIRST_PORT;
        let ports = manifest
            .ports
            .iter()
            .map(|port| {
                let host_port = if port.fixed {
                    port.default
                } else {
                    next_port += 1;
                    next_port - 1
                };
                (port.id.clone(), host_port)
            })
            .collect();
        Self {
            project: manifest.id.clone(),
            app_data: format!("{STAND_IN_ROOT}app-data"),
            puid: STAND_IN_PUID.to_owned(),
            pgid: STAND_IN_PGID.to_owned(),
            tz: STAND_IN_TZ.to_owned(),
            shares: manifest
                .shares
                .iter()
                .map(|share| (share.id.clone(), format!("{STAND_IN_ROOT}share-{}", share.id)))
                .collect(),
            ports,
            settings: manifest
                .settings
                .iter()
                .map(|setting| (setting.id.clone(), format!("{STAND_IN_SETTING}{}", setting.id)))
                .collect(),
        }
    }

    /// The variables to hand to `docker compose`, by name.
    pub fn variables(&self) -> BTreeMap<String, String> {
        let mut variables = BTreeMap::from([
            ("APP_DATA".to_owned(), self.app_data.clone()),
            ("PUID".to_owned(), self.puid.clone()),
            ("PGID".to_owned(), self.pgid.clone()),
            ("TZ".to_owned(), self.tz.clone()),
        ]);
        for (id, path) in &self.shares {
            variables.insert(Manifest::share_variable(id), path.clone());
        }
        for (id, port) in &self.ports {
            variables.insert(Manifest::port_variable(id), port.to_string());
        }
        for (id, value) in &self.settings {
            variables.insert(id.clone(), value.clone());
        }
        variables
    }

    /// Whether a host port is one of the stand-ins (fixed ports keep their number).
    pub(crate) fn is_stand_in_port(port: u16) -> bool {
        (STAND_IN_FIRST_PORT..STAND_IN_FIRST_PORT + 1000).contains(&port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::minimal;
    use crate::{Access, Port, Share};

    #[test]
    fn stand_ins_name_every_variable() {
        let mut manifest = minimal();
        manifest.ports = vec![
            Port { id: "web".into(), label: "Web".into(), default: 8080, fixed: false },
            Port { id: "sync-udp".into(), label: "Sync".into(), default: 22000, fixed: false },
        ];
        manifest.shares = vec![Share {
            id: "media".into(),
            label: "Media".into(),
            description: None,
            access: Access::ReadOnly,
        }];
        let variables = Placement::stand_in(&manifest).variables();
        assert_eq!(variables["PORT_WEB"], "61001");
        assert_eq!(variables["PORT_SYNC_UDP"], "61002");
        assert_eq!(variables["SHARE_MEDIA"], "/zendiloos-stand-in/share-media");
        assert_eq!(variables["APP_DATA"], "/zendiloos-stand-in/app-data");
        assert_eq!(variables.len(), 7);
    }
}
