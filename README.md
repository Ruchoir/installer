# Ruchoir installer

The installer, `ruchoir-install`, sets up a self-hosted Ruchoir instance à la carte. Ruchoir is a
self-hosted, European collaboration suite: chat, files, calendar, with one identity service. The
installer writes an instance's directory (its secrets and keys, the clients Propolis knows, the
proxy's configuration, a Compose file) and runs it with Docker Compose.

[Propolis](https://github.com/Ruchoir/propolis), which signs people in and holds organisations, and
the [hub](https://github.com/Ruchoir/hub), which carries notifications, real time and the instance's
manifest, are always installed, behind a Traefik reverse proxy. The products are chosen: the
[drive](https://github.com/Ruchoir/drive), the [calendar](https://github.com/Ruchoir/calendar) and
the [chat](https://github.com/Ruchoir/chat) (choosing the chat also installs the drive). The
installer keeps the manifest of installed products, which the hub publishes; it holds none of the
products' logic. How the pieces fit: [architecture](https://github.com/Ruchoir/.github/blob/main/docs/architecture.md).

## Status

`init`, `up` and `down` set up and run a production instance under your domain, with certificates
from Actalis by ACME; `dev` runs a development instance on `*.localhost`, built from the services'
repositories; `migrate` moves the data of an instance of the former single application
(`Ruchoir/ruchoir`) into a new one ([runbook](docs/migrate-monolith.md)). Not here yet: published images of the services (a production instance needs images
built from their repositories, see [deployment](docs/deployment.md#images)), a backup or restore
command, an update command, removing a product, and a certificate you bring yourself.

## Prerequisites

- Rust: the version pinned in `rust-toolchain.toml` (rustup installs it on first use), to build the
  installer.
- Docker with its Compose plugin (`docker compose`), on the machine that runs the instance.
- For the development mode: the services' repositories checked out side by side, and a GitHub token
  with the `read:packages` scope in `NODE_AUTH_TOKEN`, for the `@ruchoir` packages the pages of
  Propolis and the products are built with (GitHub Packages asks for one even to read a public
  package).

## Getting the installer

There is no prebuilt binary yet. From a checkout of this repository:

```sh
cargo install --path .
```

installs `ruchoir-install` in Cargo's `bin` directory (`~/.cargo/bin`). `cargo build --release`
builds it at `target/release/ruchoir-install` instead.

## Running it

```sh
ruchoir-install init --domain example.org --acme-email admin@example.org --dir /srv/ruchoir
ruchoir-install up --dir /srv/ruchoir
```

`init` writes the instance's directory: the secrets (`.env`) and keys (`keys/`), kept when it runs
again, and everything else written anew. It prints the hosts to point at the machine: Propolis
answers at `id.example.org`, the hub at `hub.example.org`. Your own changes (the mail relay, for
example) go in `compose.override.yaml`, which `init` never touches. `up` starts the instance and
waits until it is healthy; `down` stops it, and its data stays.

The products come with `--products`, and are kept: a later `init` without `--products` installs them
still, and none is ever removed.

- `--products drive`: the drive at `drive.example.org`, with Garage to keep its files and Euro-Office
  to edit office documents live, at `office.example.org`. The editor's engine takes about 2 GB of
  memory (4 GB at most, `OFFICE_MEM_LIMIT` in `.env` to change it), 7 GB of disk for its image, and
  a minute or two to start. `up` prepares Garage the first time, and checks it on every start.
- `--products calendar`: the calendar at `calendar.example.org`, on its own or beside the drive,
  whose files its events then carry. It mails the people invited by address.
- `--products chat`: the chat at `chat.example.org`, which always brings the drive. Beside the
  calendar, events are posted as cards in channels.

An app or a page finds what is installed at `https://hub.example.org/.well-known/ruchoir-manifest`:
the hub serves the `manifest.json` that `init` writes. What a server needs, what each file is for,
and how to add a product later: [deployment](docs/deployment.md). A production instance sets no mail
relay: [email delivery](docs/email-delivery.md).

### For development

With the `propolis` and `hub` repositories (and those of the products asked) checked out side by
side:

```sh
NODE_AUTH_TOKEN=... ruchoir-install dev --source .. --products calendar,chat
```

builds their images, starts the instance at `https://id.localhost`, and catches every mail at
`http://localhost:8025`. Trust `ruchoir-dev/dev-ca/ca.pem`, the local certificate authority, in your
browser. Without `--source`, the `ruchoir/<service>:dev` images must exist; `--no-start` writes the
directory and builds the images without starting anything. The whole path, from the checkouts to a
signed-in browser: [getting started](https://github.com/Ruchoir/.github/blob/main/docs/getting-started.md).

A development instance is its own Compose project, named after its directory and a short hash of
its path (`ruchoir-dev-1a2b3c4d`), so two directories never share their data. They still
take the same ports (443 and 8025) and network: stop one (`ruchoir-install down --dir ...`) before
starting another. Its hub wakes no phone (`HUB_PUSH_RELAY_URL` is empty): the publisher's push relay
is for real instances.

Browsers send every `*.localhost` name to this machine. Other programs ask the system, and Windows
(unlike most Linux systems) does not resolve them: a script, `curl` or the desktop app then fails on
`id.localhost`. `ruchoir-install hosts` prints a hosts-file line for each of the instance's names;
`ruchoir-install hosts --write`, from a terminal run as administrator (with `sudo` elsewhere), puts
them in the system's hosts file between two marker lines, replacing what an earlier run wrote there.
Run it again after adding a product. Those programs must also trust `ruchoir-dev/dev-ca/ca.pem`
(`NODE_EXTRA_CA_CERTS`, `SSL_CERT_FILE`, `RUCHOIR_EXTRA_CA` for the desktop app).

`--lab` adds Lab at `https://lab.localhost`, with its runner (the browser tests and the servers'
status). Lab is the team's internal tool, kept in a private
repository, so outside the team `--lab` is of no use: it needs the `lab` repository checked out
beside the others. It is never in the manifest and never installed in production. Lab lets in the
members of one organisation: once it exists in Propolis, set its id as `LAB_ORGANISATION` (in
`compose.override.yaml` or the shell) and restart Lab.

## Testing it

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The tests run `init` on throwaway directories and read what it wrote: they need no Docker. The
migration's test (`tests/migrate.rs`) also needs a PostgreSQL it may create databases in, named by
`RUCHOIR_MIGRATE_TEST_DATABASE` (`host=127.0.0.1 port=5432 user=postgres password=postgres`, no
database); without it, it says so and passes. The CI
also runs `cargo deny --config "$RULES/deny.toml" check`, with a clone of `Ruchoir/.github` at
`$RULES`.

## Settings

The installer takes its settings as options: `ruchoir-install <command> --help`. What it writes to
an instance, `.env` included, is described in [`AGENTS.md`](AGENTS.md), section "How it is built",
and in [deployment](docs/deployment.md).

## Documentation

- [`docs/deployment.md`](docs/deployment.md): running an instance for people: the server, the
  domain and its certificates, what the instance's directory holds, the images, adding a product,
  updating.
- [`docs/email-delivery.md`](docs/email-delivery.md): giving the instance a mail relay, and what
  works without one.
- [`docs/backup.md`](docs/backup.md): what to save and how to put it back.
- [`docs/migrate-monolith.md`](docs/migrate-monolith.md): moving an instance of the former single
  application to the services, what is carried over and what is not, checking, going back.
- [`docs/adr/`](docs/adr/README.md): this repository's decisions.
- [`AGENTS.md`](AGENTS.md): how to work here, written for contributors and their tools: the common
  rules of the organization, how the installer is built, its gotchas and its checks.
- The decisions that span several repositories, among them [0004. The installer and the product
  manifest](https://github.com/Ruchoir/.github/blob/main/docs/adr/0004-installer-and-product-manifest.md):
  [Ruchoir/.github, `docs/adr`](https://github.com/Ruchoir/.github/tree/main/docs/adr).

## Contributing

Contributions are welcome. Read [`CONTRIBUTING.md`](https://github.com/Ruchoir/.github/blob/main/CONTRIBUTING.md):
your first pull request asks you to sign the [contributor licence agreement](https://github.com/Ruchoir/.github/blob/main/CLA.md).
Report a vulnerability privately, never in an issue: [`SECURITY.md`](https://github.com/Ruchoir/.github/blob/main/SECURITY.md).

## Licence

`AGPL-3.0-only` (`LICENSE`).
