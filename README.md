# zendiloOS Community Apps

The app catalog of [zendiloOS](https://github.com/mavnezz/zendiloOS): apps
the administrator can install on the NAS with a click. Each app is a Docker
Compose project under a policy that keeps it from taking over the machine:
no privileged containers, the Docker socket only with the administrator's
consent, data only in the app's own folder and the shares the administrator
picks.

> **Status:** format version 1, ahead of zendiloOS's app support
> (milestone M6). The catalog can be validated and published; no zendiloOS
> release installs from it yet.

## Apps

Every folder in [`apps/`](apps) is one app, with three files:

```text
apps/jellyfin/
├── app.yaml       what the app is and what it needs
├── compose.yaml   how Docker runs it
└── icon.png       256×256 PNG
```

[docs/format.md](docs/format.md) describes the format, the variables
zendiloOS fills in and everything the policy allows.

## Adding or Changing an App

Through a pull request; see [CONTRIBUTING.md](CONTRIBUTING.md). CI checks
every app against the format and the policy, and asks the registries whether
the images exist. A maintainer reviews and merges.

## Tools

The validator lives in [`tools/`](tools): the crate `zendiloos-catalog`
holds the format and the policy (pure, so the zendiloOS agent can run the
same checks on the NAS), the binary `catalog` reads the app folders and asks
Docker. It needs Rust (the version in `rust-toolchain.toml`), Docker with the
Compose and Buildx plugins, and [just](https://github.com/casey/just).

```sh
just validate jellyfin   # check one app, including the registries
just validate-offline    # check all apps without the network
just index               # build dist/index.json and the icons
just check               # everything CI checks
```

## Publishing

After every push to `main`, CI publishes `index.json` and the icons to
GitHub Pages under `/v1/`; zendiloOS downloads them from there. Apps follow
moving tags such as `latest`, so zendiloOS finds new versions in the
registries; Renovate opens pull requests for new major versions of
databases and for the GitHub Actions.

## License

[Apache-2.0](LICENSE). App icons are the logos of their projects and belong
to them.
