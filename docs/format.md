# App Format

> Format version 1. The validator in `tools/` enforces everything described
> here; `just validate` runs it.

An app is a folder `apps/<id>/` with exactly three files:

| File | Content |
|---|---|
| `app.yaml` | what the app is and what it needs from the NAS (the manifest) |
| `compose.yaml` | how Docker runs it, as a Compose file with variables zendiloOS fills in |
| `icon.png` | square PNG, 256 to 1024 pixels, at most 256 KiB |

Nothing else belongs in the folder: Docker would read some files on its own
(`.env`, `compose.override.yaml`).

## 1. How zendiloOS installs an app

An app of the catalog is a template for a Docker Compose project of the
administrator's own; once installed, the project is theirs.

1. The administrator picks an app and a place for it: a project name (the
   app's id by default) and a folder in a share (`docker/<id>` by default),
   which becomes the project's folder.
2. zendiloOS shows the manifest: the permissions, the shares to choose, the
   settings to fill in and the host ports it proposes. It fills the
   [variables](#3-variables) into `compose.yaml` itself, as literal values
   (quoted where YAML needs it, a `$` doubled for Compose), keeping the
   file's comments and layout, and shows the result, which may still be
   changed.
3. The file then takes the path of any project an administrator writes:
   Compose reads it, what reaches beyond the containers (devices, extra
   capabilities, the host network, the Docker socket, …) is named and must
   be confirmed, the file is written as `compose.yaml` into the project's
   folder, the folders it binds inside that folder are made as the
   administrator, and the project is deployed.
4. From then on it is a project like any other in zendiloOS's Docker app:
   its file is edited and deployed there, its images updated there. The App
   Center only knows which app it came from, to open its web interface.

The catalog's CI runs the [policy](#4-what-compose-files-may-do) on every
pull request, and zendiloOS runs the same checks on every entry of an index
before it lists the app: an app that fails them is not offered.

## 2. `app.yaml`

```yaml
id: jellyfin                      # = folder name; lower-case letters, digits, hyphens
name: Jellyfin
version: "12.4"                   # optional; apps on a moving tag such as latest leave it out
tagline: Stream your films, series and music to every device   # one line, ≤ 80
description: |                    # ≤ 3000 characters
  Jellyfin is a free media server …
categories: [media]               # one to three, see below
developer: Jellyfin contributors  # who makes the app
website: https://jellyfin.org
source: https://github.com/jellyfin/jellyfin
license: GPL-2.0-only             # SPDX expression of the app's licence
maintainers: [mavnezz]            # GitHub accounts that look after this entry
architectures: [amd64, arm64]     # must include amd64
notes: |                          # optional: shown before installing
  Create the first account in the setup wizard …
web:                              # optional: the interface zendiloOS opens
  port: web                       # id of a port below
  path: /                         # default /
  scheme: http                    # http (default) or https
ports:
  - id: web
    label: Web interface
    default: 8096                 # host port zendiloOS proposes
shares:
  - id: media
    label: Media library
    description: The share with your films, series and music.
    access: read-only             # or read-write
settings:
  - id: DB_PASSWORD
    label: Database password
    type: password
    generate: true                # zendiloOS generates it; nobody is asked
  - id: SECRET_ENCRYPTION_KEY
    label: Encryption key
    type: password
    generate: true
    length: 64                    # optional, 16 to 128; 32 when left out
    alphabet: hex                 # optional: alphanumeric (default) or hex
  - id: ADMIN_EMAIL
    label: Administrator e-mail
    type: text                    # text, password, number, boolean
    required: true
    default: admin@example.org    # optional; never for passwords
permissions:                      # exceptions the administrator confirms
  devices: [/dev/dri, /dev/ttyUSB0]
  capabilities: [NET_ADMIN]
  host_network: true
  docker_socket: true
translations:
  de:                             # ISO 639-1; English is the default
    name: …                       # each field optional
    tagline: …
    description: …
    notes: …
```

Unknown fields are errors, so typos do not go unnoticed.

**Categories:** `ai`, `backup`, `communication`, `development`,
`downloads`, `files`, `finance`, `games`, `home-automation`, `media`,
`monitoring`, `network`, `photos`, `productivity`, `security`.

**Host ports** may not be ones the NAS uses itself: 22, 80, 111, 137, 138,
139, 443, 445, 2049, 3702, 5353.

## 3. Variables

zendiloOS provides these and nothing else; the validator refuses any other
`${…}` in `compose.yaml`.

| Variable | Value | May be used in |
|---|---|---|
| `APP_DATA` | the project's folder, written as `.`: `${APP_DATA}/config` becomes `./config` | source of a bind mount |
| `SHARE_<ID>` | the path of the share, or the folder in it, the administrator picked for `shares[].id` (upper-case, `-` → `_`), e.g. `/volume1/media/Films` | source of a bind mount |
| `PORT_<ID>` | the host port for `ports[].id` | `ports` (host and container side), `expose`, `environment`, `command`, `entrypoint`, `healthcheck` |
| `PUID`, `PGID` | the installing administrator's account and primary group; the folders below `APP_DATA` are theirs | `environment`, `user` |
| `TZ` | the NAS's time zone, e.g. `Europe/Berlin` | `environment` |
| `<SETTING>` | the value of a setting | `environment`, `command`, `entrypoint`, `healthcheck` |

The column on the right is enforced: a value the administrator types in must
not end up in an `image` or a mount.

Mount folders below `APP_DATA` (`${APP_DATA}/config`), never `APP_DATA`
itself: that is the project's folder, with `compose.yaml` in it. Publish the
web interface's port in the short form `${PORT_<ID>}:<container port>`:
zendiloOS finds the app's address again by the container port it publishes.
Write each device as an item of its own in a block list (`- /dev/dri:/dev/dri`):
zendiloOS comments out the items of devices the machine lacks, and only
such items.

The values stand in the file as they are, generated secrets too: whoever may
read the share the project's folder is in may read them.

## 4. What compose files may do

Docker access is root on the NAS, so the policy allows by name rather than
forbids by name: a key that Compose learns in a future version is refused
until someone has looked at what it does.

**Images**

- Every service names an `image`, from any registry and with any tag. An
  app follows the tag its developers recommend, usually `latest`
  (`lscr.io/linuxserver/jellyfin:latest`; without a tag Docker takes
  `latest`), so new versions reach the NAS without a change to the catalog.
- Databases and other services whose data only one major version can read
  follow their major tag instead (`postgres:18-alpine`, `valkey/valkey:9`):
  `latest` would one day jump to a version that cannot open the data.
- A digest (`name:tag@sha256:…`) is allowed for an image that must not move.
- Settings cannot end up in `image`: the administrator picks values, not
  images.
- No `build`: zendiloOS does not build images.

**Data**

- Bind mounts only from `${APP_DATA}` (and folders below it) or a declared
  share `${SHARE_<ID>}` as a whole. A share declared `read-only` must be
  mounted `:ro`.
- No Docker volumes, named or anonymous: their data would live on the system
  disk, without snapshots or backups.
- No mount inside another mount's folder (`${APP_DATA}` and
  `${APP_DATA}/config` together): a container could replace the inner folder
  with a link, and Docker would follow it the next time it starts the app.
- `tmpfs` is fine.
- The Docker socket (`/var/run/docker.sock`) with the permission
  `docker_socket` (see 5).
- Not `/etc/localtime`: use `TZ: ${TZ}`.

**Network**

- Publish ports as `${PORT_<ID>}:<container port>`, without a host address.
  Each declared port is published at least once and at most once per
  protocol, so an app that listens on one number for TCP and UDP declares
  one port and publishes it twice (`${PORT_PEER}:6881` and
  `${PORT_PEER}:6881/udp`).
- An app that must listen on the same port it is reached at (it puts the
  port into links, or checks it) uses the variable on both sides and is told
  the port through its environment: `${PORT_WEB}:${PORT_WEB}` with
  `WEBUI_PORT: ${PORT_WEB}`.
- The app's own networks only: no `external`, only the `bridge` driver, no
  fixed `ipam` ranges, no network `name`.
- `network_mode` may be `none`, `service:<name>` of the same app, or `host`
  with the permission (see below).

**Secrets**

- No passwords, secrets, tokens or keys as plain text: a variable named like
  `*PASSWORD*`, `*SECRET*`, `*TOKEN*`, `*API_KEY*` or `*_KEY` must get its
  value from a setting. Passwords the app's own services share (a database
  password) are settings with `generate: true`; zendiloOS generates them
  once, when installing, and keeps them across updates. A generated value is
  32 letters and digits (`A–Z`, `a–z`, `0–9`), so it is safe in connection
  strings, URLs and shell commands; `length` and `alphabet: hex` fit it to
  apps that expect, say, 64 hex digits.

**Everything else**

- `restart` on every service: `unless-stopped`, or `"no"` for a job that
  runs once.
- Refused: `privileged`, `pid`, `uts`, `userns_mode`, `cgroup`,
  `cgroup_parent`, `device_cgroup_rules`, `gpus`, `runtime`,
  `volumes_from`, `container_name`, `sysctls`, `use_api_socket`, `provider`,
  `post_start`, `pre_stop`, `external_links`, `oom_kill_disable`,
  `env_file`, `label_file`, `extends`, `include`, `secrets`, `configs`.
- `security_opt` only `no-new-privileges:true`; `ipc` only `private`,
  `shareable` or `service:<name>`; logging only `json-file` or `local`;
  `deploy` only `resources` (cpus, memory, pids); labels must not start with
  `com.docker.` or `zendiloos.`.
- Allowed as they are: `command`, `entrypoint`, `environment`, `user`,
  `working_dir`, `hostname`, `healthcheck`, `depends_on`, `expose`, `tmpfs`,
  `cap_drop`, `init`, `read_only`, resource limits, `ulimits`, `extra_hosts`,
  `dns*`, `group_add`, `shm_size`, `stop_signal`, `stop_grace_period`,
  `platform`, `pull_policy`, `x-*`.

## 5. Permissions

Exceptions from the policy are declared under `permissions` and confirmed by
the administrator when installing. Each one declared must be used, and each
one used must be declared.

| Permission | Compose | Use |
|---|---|---|
| `devices: [<path>, …]` | `devices: [<path>:<path>]` | devices of the kinds below |
| `capabilities: [NET_ADMIN]` | `cap_add: [NET_ADMIN]` | VPNs, DHCP servers |
| `capabilities: [SYS_NICE]` | `cap_add: [SYS_NICE]` | real-time priority for media and audio apps |
| `host_network: true` | `network_mode: host` | discovery on the home network (mDNS, SSDP, DLNA, HomeKit) |
| `docker_socket: true` | `- /var/run/docker.sock:/var/run/docker.sock` | dashboards and tools that show or manage containers |

**Docker socket.** Whoever can reach the socket can start a container with
the NAS's root filesystem inside: the permission hands the app control over
the whole NAS. zendiloOS says so plainly when it asks the administrator.

**Devices.** `permissions.devices` lists each device path the compose file
uses, or a whole folder such as `/dev/dri`. These kinds are available
(`*` stands for a number):

| Path | For |
|---|---|
| `/dev/dri` | video acceleration (Intel Quick Sync, AMD and Intel VAAPI) |
| `/dev/net/tun` | VPN tunnels |
| `/dev/ttyUSB*`, `/dev/ttyACM*` | Zigbee and Z-Wave sticks, UPS, 3D printers |
| `/dev/ttyS*`, `/dev/ttyAMA*` | serial ports |
| `/dev/serial/by-id/…` | serial devices by their stable name |
| `/dev/video*` | cameras and capture cards |
| `/dev/snd` | sound cards |
| `/dev/apex_*` | Coral Edge TPUs |

Disks and partitions, memory (`/dev/mem`), consoles and the like are not
available: each would hand over the NAS itself. zendiloOS passes a device
only when the machine has it, so an app that can use a GPU or a Zigbee stick
still runs without one; an app that should find a stick under several names
lists them all (`/dev/ttyUSB0`, `/dev/ttyUSB1`, `/dev/ttyACM0`, …).

Docker's default capabilities (`CHOWN`, `SETUID`, …) may be added back after
`cap_drop: [ALL]` without permission.

**Host network.** A service with `network_mode: host` sees every interface
of the NAS and listens on its ports directly, so it publishes nothing and
zendiloOS cannot move its ports. Declare those ports in `app.yaml` with
`fixed: true`; zendiloOS checks they are free before installing:

```yaml
ports:
  - id: web
    label: Web interface
    default: 8123
    fixed: true
permissions:
  host_network: true
```

**Reaching other apps.** Each app has its own network, so apps reach one
another through the NAS, on the host ports zendiloOS gave them. Add
`extra_hosts: ["host.docker.internal:host-gateway"]` to the service and let
the administrator enter the address as a setting, with the other app's
default port as default:

```yaml
settings:
  - id: RADARR_URL
    label: Radarr address
    type: text
    default: http://host.docker.internal:7878
```

## 6. Updates

Images follow moving tags, so a new version of an app needs no change to the
catalog: zendiloOS's Docker app looks for new images under the tags a
project uses and pulls them when asked or on its schedule. A changed entry
in the catalog reaches only new installations; an installed app's file
belongs to the administrator, and zendiloOS does not change it. The catalog
itself changes through pull requests only.

Renovate opens a pull request when an image on a major tag (a database) gets
a new major version; whoever reviews it checks the release notes for
migrations.

## 7. The index

On every push to `main`, CI builds `index.json` and `icons/<id>.png` with
`catalog index --out <dir>`:

```json
{
  "format": 1,
  "apps": [
    {
      "id": "jellyfin",
      "name": "Jellyfin",
      "…": "every field of app.yaml",
      "compose": "services:\n  jellyfin:\n …",
      "icon": "icons/jellyfin.png",
      "digest": "sha256:…"
    }
  ]
}
```

`compose` is the file as written; zendiloOS interpolates it on the NAS.
`digest` covers all three files, so it changes whenever the app's definition
does. `format` rises when older zendiloOS versions could no longer read the
index.

## 8. What zendiloOS must do on the NAS

The checks live in the `zendiloos-catalog` crate. zendiloOS:

- checks every entry of an index with them before it lists the app, and
  leaves out an entry that fails, never repairing it;
- fills the variables into the file itself, refusing values with line
  breaks or longer than 4096 bytes, and parses the result again: it must be
  the same document with the values in place;
- treats the result as an administrator's own project from there on (its
  own checks and confirmation, what runs is what was confirmed, binds into
  a volume pinned so a link swapped in later is not followed);
- makes the folders below `APP_DATA` as the administrator before deploying,
  opening each name without following links;
- comments out devices the machine lacks;
- generates `generate: true` settings once, as 32 letters and digits or as
  `length` and `alphabet` say;
- serves icons with `Content-Type: image/png` and does not trust the index
  beyond what the checks allow.

Apps zendiloOS installed the earlier way (with a dataset and an account of
their own, run from a normalized file the agent kept) keep running and can
be removed; new installations take the path above.
