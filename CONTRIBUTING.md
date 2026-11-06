# Contributing an App

Apps come into the catalog through pull requests. A good pull request adds
or changes one app.

## Before You Start

The catalog takes apps that

- are maintained and published as a Docker image by their developers or a
  well-known packager (such as linuxserver.io),
- are useful on a NAS in a home or small office,
- work within the [policy](docs/format.md#4-what-compose-files-may-do): no
  privileged containers, data only below `${APP_DATA}` and in shares the
  administrator picks; devices, the host network and the Docker socket only
  as permissions the administrator confirms.

Apps that need the NAS's own ports 80 and 443 (reverse proxies) or
privileged mode (Netdata, WireGuard with kernel modules) cannot be in the
catalog.

## Adding an App

1. Fork the repository and create `apps/<id>/`, where `<id>` is lower-case
   letters, digits and hyphens, such as `uptime-kuma`.
2. Write `app.yaml` and `compose.yaml` as described in
   [docs/format.md](docs/format.md); [`apps/jellyfin`](apps/jellyfin) is a
   short example. In short:
   - use the image tag the developers recommend, usually `latest`;
     databases follow their major tag (`postgres:18-alpine`)
   - app data below `${APP_DATA}`, user files from declared shares
     (`${SHARE_<ID>}`), read-only unless the app must write
   - publish ports as `${PORT_<ID>}:<container port>`
   - no passwords in the file: database passwords and the like are settings
     with `generate: true`
   - `restart: unless-stopped`
   - descriptions in plain English, a German translation is welcome
3. Add `icon.png`: square, 256×256 (up to 1024), at most 256 KiB.
4. Check it: `just validate <id>`. Without Rust or Docker at hand, open the
   pull request anyway; CI runs the same checks and comments on each
   problem.
5. Open a pull request. Say how you tried the app.

See which architectures an image has with:

```sh
docker buildx imagetools inspect lscr.io/linuxserver/jellyfin:latest
```

They must include everything `architectures` names in `app.yaml`.

## Changing an App

Apps follow moving tags, so new versions reach the NAS without a pull
request. When an app changes in a way the compose file must follow (mount
points, variables, ports), update the app. Renovate opens a pull request
when a database on a major tag gets a new major version; whoever reviews it
checks the release notes for migrations.

Other changes: the same way as adding an app. Never rename an app's `id`,
shares, ports or settings without need: installed apps keep their data and
choices by these names.

## Review

A maintainer checks what CI cannot:

- the image comes from the project or a well-known packager;
- shares are read-only where the app does not need to write;
- permissions (`devices`, `capabilities`, `host_network`) are really needed,
  and `notes` say what the administrator must know;
- the app starts with the given values: data folders owned by `PUID:PGID`,
  the web interface on the declared port.
