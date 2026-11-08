# Security policy

What this catalog lists, a NAS installs with a click, so reports of
vulnerabilities are welcome and handled before anything else.

## Reporting a vulnerability

Please report it privately through GitHub: **Security → Report a
vulnerability** in this repository. Do not open a public issue or pull
request for it.

Reports belong here when they concern the catalog itself:

- an app that does more than its `app.yaml` admits: a hijacked or
  malicious image, a secret written into its files, a way around the
  permissions the administrator confirms;
- a compose file the validator accepts although the
  [policy](docs/format.md#4-what-compose-files-may-do) refuses it;
- a way to change the published index without a reviewed pull request.

A useful report names the app or the check, shows how to reproduce it
(for a validator bypass: a compose file that `just validate` accepts) and
says what an attacker gains.

A vulnerability in an app's own software goes to its developers, one in
zendiloOS to [its repository](https://github.com/mavnezz/zendiloOS/security).

You will get an answer within a few days. A fix is live once it is merged
into `main` and the index is published again.

## Supported versions

Only `main`: zendiloOS downloads the index built from it.
