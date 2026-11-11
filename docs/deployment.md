# Deployment

Moved from `docs/deployment.md` of the former monolith (`Ruchoir/ruchoir`) on 2026-11-11, and brought up to date with this repository.

Putting a Ruchoir instance on a server with `ruchoir-install`: a home server, a VPS, a box in a
cupboard. The [README](../README.md) says how to get the installer and how to run a development
instance; this page covers running one for people.

## What you need first

- A machine with Docker and its Compose plugin (`docker compose`). On Linux, the services run as the
  owner of the instance's directory (`RUCHOIR_USER` in `.env`), and its private files are readable by
  that owner alone.
- **A domain name you control**, and a DNS record for each host of the instance, pointing at the
  machine: `id.<domain>` (Propolis) and `hub.<domain>` always, then `drive.<domain>` and
  `office.<domain>` with the drive, `calendar.<domain>` and `chat.<domain>` with theirs. `init` prints
  the list.
- **Ports 80 and 443 reachable from the internet** at those names. The proxy asks Actalis (Italy) for
  the certificates by ACME, with the TLS challenge on port 443, and sends every request on port 80 to
  HTTPS. The installer sets up no other challenge, and no certificate you bring yourself yet, so an
  instance reachable only on a private network cannot get its certificates this way.
- With the drive, room for the office engine: about 2 GB of memory at rest, capped at 4 GB
  (`OFFICE_MEM_LIMIT` in `.env`), and 7 GB of disk for its image.

## HTTPS is not optional

Propolis and the products keep their sessions in `__Host-` cookies, which browsers only accept over
HTTPS, and passkeys need a secure context too. The installer takes care of it: Traefik terminates
TLS for every host with the Actalis certificates, and every answer tells browsers to use HTTPS for
two years, subdomains included. Traefik is the instance's only proxy, and the services believe the
client address it forwards because it is on the instance's own network.

## Setting it up

```sh
ruchoir-install init --domain example.org --acme-email admin@example.org --products drive,calendar,chat --dir /srv/ruchoir
ruchoir-install up --dir /srv/ruchoir
```

`--acme-email` is the address Actalis writes to about the certificates. `--products` is optional:
Propolis and the hub are always installed, and the products are `drive`, `calendar` and `chat`
(which brings the drive).

`init` writes the instance's directory:

| Path | What |
|---|---|
| `.env` | The secrets, made once and kept: the database passwords, Propolis's encryption key (`PROPOLIS_SECRET_KEY`), and with the products theirs (the calendar's `CALENDAR_SECRET_KEY`, Garage's secrets, the drive's S3 key) |
| `keys/` | Each service's Ed25519 keys, made once and kept: `<service>-auth.pem` to authenticate to Propolis, `<service>-events.pem` to sign its events, each with its id beside it; and the hub's VAPID key for web push, `hub-vapid.pem` |
| `manifest.json` | The services installed and their addresses, served by the hub at `https://hub.<domain>/.well-known/ruchoir-manifest` |
| `compose.yaml` | Every service, run together |
| `propolis/clients.toml` | The clients Propolis knows: the hub, each product, and Ruchoir's desktop and mobile apps. Public keys only |
| `hub/subscriptions.toml` | Which product receives which events |
| `postgres/init.sh` | One database and one role per service, run once, on an empty data directory |
| `traefik/` | The proxy's configuration and routes |
| `garage/garage.toml`, `office/` | With the drive: its object store's configuration, and the files the office engine mounts |
| `compose.override.yaml` | Yours: your own changes, which `init` never touches |

Running `init` again never replaces a secret or a key, since the services and their data depend on
them, and writes every other file anew. Change the instance through `init` and
`compose.override.yaml`, never by editing the files it writes.

`up` runs `docker compose up --detach --wait` in the directory, then, with the drive, prepares Garage
(its layout, the drive's key and bucket), which is safe on every start. `down` runs
`docker compose down`: the containers go, the data stays in its volumes.

The data lives in Docker volumes: `postgres-data` (one database per service), `valkey-data`
(sessions and other short-lived state), `acme` (the certificates), and with the drive `garage-meta`
and `garage-data` (the files). The Compose project is called `ruchoir` whatever the directory, so the
volumes are `ruchoir_postgres-data` and so on, and one machine runs one instance, a development
instance included. [Backup and restore](backup.md) says what to save.

## Images

PostgreSQL, Valkey, Traefik and Garage come from Docker Hub, and the office engine from the GitHub
container registry. Propolis, the hub and the products are named
`registry.ruchoir.com/<service>:latest`, but their repositories do not publish images yet. Until
they do, build each one from its repository, as `ruchoir-install dev --source` does, and name it in
`.env`, which `init` keeps:

```sh
docker build --secret id=npm_token,env=NODE_AUTH_TOKEN --tag ruchoir/propolis:local propolis
echo PROPOLIS_IMAGE=ruchoir/propolis:local >> /srv/ruchoir/.env
```

The secret, a GitHub token with `read:packages` in `NODE_AUTH_TOKEN`, is for Propolis and the
products, whose pages are built with the `@ruchoir` packages; the hub needs none. The variables are
`PROPOLIS_IMAGE`, `HUB_IMAGE`, `DRIVE_IMAGE`, `CALENDAR_IMAGE` and `CHAT_IMAGE`.

## The first account

Open `https://id.<domain>/sign-up/` and create an account. Registration is open: anyone who reaches
Propolis can create an account, and it opens once its address is confirmed. A production instance
has no mail relay until you give it one, so the confirmation is then only in Propolis's log
(`docker compose logs propolis`, in the directory). [Email delivery](email-delivery.md) covers the
relay, and what works without one.

## Checking it worked

- `up` returns once every service with a health check answers it. `docker compose ps`, in the
  directory, shows each service's state.
- `https://hub.<domain>/.well-known/ruchoir-manifest` lists the services installed, and
  `https://id.<domain>/.well-known/openid-configuration` answers with Propolis's discovery document.
- `https://id.<domain>/api/v1/health` says whether Propolis reaches its database and its Valkey.
- `docker compose logs propolis`: `client registry applied` means Propolis read the clients `init`
  wrote. The installer gives Propolis no breached-password filter, so it warns that this check is off.

## Adding a product later

```sh
ruchoir-install init --domain example.org --acme-email admin@example.org --products calendar --dir /srv/ruchoir
```

adds the calendar and keeps what was installed. Give the same domain and address as the first time:
`init` writes the proxy's configuration from them again. Point the new host at the machine. Then,
because `postgres/init.sh` only runs on an empty data directory, create the product's role and
database by hand before starting it, with the statements of `postgres/init.sh` and the password
`init` added to `.env` (for each product added: the chat brings the drive):

```sh
cd /srv/ruchoir
docker compose exec postgres psql -U ruchoir
```

```sql
CREATE ROLE calendar LOGIN PASSWORD '<CALENDAR_DB_PASSWORD from .env>';
CREATE DATABASE calendar OWNER calendar;
```

then `ruchoir-install up --dir /srv/ruchoir`. A product cannot be removed yet.

## Updating

There is no update command yet. To move to newer images, build them again (or `docker compose pull`
in the directory, once they are published) and run `ruchoir-install up`: Compose recreates the
services whose image or settings changed. A newer installer is applied by running its `init` again
with the same domain and address, then `up`. Take a [backup](backup.md) first.

## Not there yet

Deliberate gaps, so nobody discovers them the hard way:

- **No backup or restore command.** [Backup and restore](backup.md) says what to save, with standard
  commands.
- **No published images, no update command, no removal of a product.** See above.
- **No certificate of your own.** Certificates come from Actalis by the TLS challenge only.
- **A product added to an instance that already ran needs its database made by hand.** See above.
- **No mail relay by default.** See [email delivery](email-delivery.md).
