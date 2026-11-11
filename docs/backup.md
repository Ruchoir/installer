# Backup and restore

Moved from `docs/backup.md` of the former monolith (`Ruchoir/ruchoir`) on 2026-11-11, and brought up to date with this repository.

The installer has no backup or restore command yet. This page says what an instance it set up is
made of, what to save, and how, with standard Docker commands.

## What to save

An instance is its directory and its Docker volumes, and a backup that misses one of them is not a
backup:

| What | What is in it | What losing it costs |
|---|---|---|
| The instance's directory | `.env` (the database passwords, Propolis's encryption key, the calendar's key, Garage's secrets and the drive's S3 key), `keys/` (each service's keys, the hub's web push key), `manifest.json` (what is installed), `compose.override.yaml` (your changes) | The data becomes unreadable or unreachable: the TOTP secrets and Propolis's signing keys are encrypted with its key, the services prove who they are with their keys, the browsers' push subscriptions belong to the hub's key |
| `postgres-data` | One database per service: accounts and organisations in Propolis, notifications in the hub, the records of the drive, the calendar and the chat | Everything |
| `garage-meta`, `garage-data` (with the drive) | The bytes of the files, and Garage's index of them | Every document, while the databases still list them |
| `valkey-data` | Sessions, sign-in throttles, short-lived codes | Everyone is signed out; nothing is destroyed |
| `acme` | The certificates and the ACME account | Nothing lasting: the proxy asks for them again |

Compose prefixes each volume with the project's name: `ruchoir_postgres-data`, `ruchoir_garage-data`,
and so on. `docker volume ls --filter name=ruchoir_` lists them.

The directory holds the instance's secrets in clear. Keep its copy, and the archives below, encrypted
(for example with `gpg --symmetric`), and keep the key somewhere that is not this machine: an archive
encrypted with a key that burns in the same fire is a compressed copy of the problem.

## Taking one

The simplest consistent backup is taken with the instance stopped: nothing can be written between
the copy of the first volume and the last, so the archive is one restore point rather than several
snapshots of different moments. The pause lasts as long as the copy.

```sh
cd /srv/ruchoir
backup=/var/backups/ruchoir/$(date +%Y%m%dT%H%M%S)
mkdir -p "$backup"
ruchoir-install down --dir .
for volume in postgres-data garage-meta garage-data valkey-data acme; do
  docker run --rm -v "ruchoir_$volume:/volume:ro" -v "$backup:/backup" debian:bookworm-slim \
    tar --numeric-owner -czf "/backup/$volume.tar.gz" -C /volume .
done
tar -czf "$backup/instance.tar.gz" -C /srv ruchoir
ruchoir-install up --dir .
```

Without the drive, leave out `garage-meta` and `garage-data`. `ruchoir-install down` stops the
containers and keeps the volumes; `up` starts them again, and checks Garage as on any start.

## Putting one back

On the same machine or a new one, with Docker and the installer, and the same domain pointing at it,
`backup` being the directory that holds the archives. **A restore replaces**: there is no merge and
no undo. On a machine where the instance already ran, stop it (`ruchoir-install down`) and remove its
volumes (`docker volume rm`) first, since they must be empty.

1. Put the instance's directory back where it was, as the user who will own it
   (`tar -xzf "$backup/instance.tar.gz" -C /srv`), so `.env`, `keys/` and `manifest.json` are those
   of the backup. Then run `ruchoir-install init` again with the same domain and ACME address: it
   keeps the secrets and the keys, reads the products from the manifest, and on Linux makes the
   directory's new owner the user the services run as.
2. Create the instance's containers and volumes without starting them, then fill each volume from
   its archive:

   ```sh
   cd /srv/ruchoir
   docker compose up --no-start
   for volume in postgres-data garage-meta garage-data valkey-data acme; do
     docker run --rm -v "ruchoir_$volume:/volume" -v "$backup:/backup:ro" debian:bookworm-slim \
       tar --numeric-owner -xzf "/backup/$volume.tar.gz" -C /volume
   done
   ```

3. `ruchoir-install up --dir /srv/ruchoir`.

PostgreSQL's files are copied as they are, so they come back under the same major version of
PostgreSQL (18, as `compose.yaml` names it). Everyone signed in since the backup is signed out,
because the sessions come back as they were at that moment.

## Checking that it works

A backup nobody has restored is a hypothesis. Replay a restore somewhere else at least once, and again
whenever the instance's versions move: copy the archives and the key to a scratch machine, put them
back as above, and open the instance.

## What this does not cover

- **A backup command.** The installer has none yet; the commands above are plain Docker, to run by
  hand or from a timer of your own.
- **Off-site copies.** The archives land on this machine. Copy them somewhere else (another machine,
  a European object store) and keep the key somewhere else again.
- **Point-in-time recovery.** You go back to an archive, not to an arbitrary minute.
- **Backups without a pause.** A copy taken while the instance runs may not be consistent between
  the databases and the drive's files.
