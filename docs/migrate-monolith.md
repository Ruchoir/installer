# Migrating an instance of the monolith

An instance of the former single application (`Ruchoir/ruchoir`, "the monolith") is moved to an
instance of the services once, with `ruchoir-install migrate`: accounts and organisations go to
Propolis, the inbox, push and notification settings to the hub, and each product's records and files
to the drive, the calendar and the chat. This page is the operator's runbook. What goes where, table
by table, and what is left behind, is documented in `src/migrate/` (one module per service).

The migration is offline: the monolith is stopped, its database dumped and its object store copied,
and the new instance is filled from those copies before anyone uses it. Nothing of the monolith is
written, so going back is starting it again.

## What is carried over, and what is not

Carried over, with their ids (links to a file or a message keep leading to it):

- accounts, with their passwords, two-factor secrets and passkeys; organisations, members, guests
  and their end dates, roles, groups, open invitations, the organisation journal;
- files and folders, every version and its bytes, thumbnails, public links, stars, the rules on
  files; the files of messages and events;
- calendars, events, exceptions, attendees (people invited by address keep their link), answers,
  reminders, shares, iCal subscriptions;
- channels, direct conversations, members, messages and threads, mentions, reactions, pins, saved
  messages, read positions, link previews, the rules on channels;
- the inbox (notifications about messages), phones' push devices, browsers' push subscriptions (with
  the monolith's VAPID key), notification settings, levels per organisation, chosen presence.

Left behind, and what it means for people:

| What | Why | For people |
|---|---|---|
| Sessions and tokens | the new instance issues its own | everyone signs in again, on the web and in the apps |
| Recovery codes | kept as an HMAC under the monolith's key, which Propolis cannot check | those with two-factor make new codes |
| Profile pictures, organisation icons | Propolis keeps no pictures yet | they are gone until it does |
| Calendar notifications | reminders that have passed, invitations each event shows | none |
| Event cards in channels | the card format is the products' now | the message stays, without its card |
| Rules for "the readers of a channel" on files | no product answers for another's audience | share the folder again with a group or people |
| The channel of a channel invitation | no product reads an invitation's context yet | invited people join the channel by hand |
| Interface preferences (theme, font, layout) | each product's pages keep their own | chosen again |
| Legacy file shares, licences, import jobs, instance settings and events | unused, or the instance's own | none |

The report of each run counts every one of these.

## What you need

- The monolith's server, with its compose directory and `.env`.
- The installer built on that server (`cargo install --path .` from a checkout of this repository),
  Docker with Compose, about twice the size of the monolith's data free on disk.
- `rclone` (or the `rclone/rclone` image) to copy the object store.
- The DNS names of the new instance (`id.`, `hub.`, `drive.`, `office.`, `calendar.`, `chat.` under
  its domain) pointing at the server, and ports 80 and 443 free once the monolith is stopped (a
  reverse proxy of the host that served the monolith is stopped too, or the new instance's proxy
  cannot start).

## Before the day

1. **Check the monolith's Compose project name**: `docker compose ls` in its directory. The installer
   names its production project `ruchoir`, so its volumes are `ruchoir_postgres-data` and so on. If
   the monolith's project is also `ruchoir`, the two would share volumes: see "When the monolith's
   project is `ruchoir`" below and do it first.
2. **Rehearse** on a copy: take a dump and a copy of the objects as below (the monolith keeps
   running), then run the whole procedure on another machine or in a development instance
   (`ruchoir-install dev`) with `--dry-run` first, then for real, and look at the result. The dry run
   reads and writes everything inside transactions it rolls back, so it finds the rows the new
   schemas refuse.
3. Tell the testers when it happens, that they will sign in again (and make new recovery codes if
   they use two-factor), and that the apps must be pointed at the new address.

## The day

In the monolith's directory (`$MONOLITH`), with its `.env`:

```sh
cd "$MONOLITH"
set -a; . ./.env; set +a
mkdir -p /srv/migration
# 1. Stop what writes; keep the database and the object store running.
docker compose stop api office
# 2. The database, in pg_dump's custom format.
docker compose exec -T postgres pg_dump -U "${POSTGRES_USER:-ruchoir}" -d "${POSTGRES_DB:-ruchoir}" -Fc   > /srv/migration/monolith.dump
# 3. The object store, each object at its key. The monolith publishes Garage's S3 port on the host
#    (GARAGE_S3_HOST_PORT, 3900 by default); its key and bucket are in .env (S3_ACCESS_KEY_ID,
#    S3_SECRET_ACCESS_KEY, S3_BUCKET).
rclone copy ":s3:${S3_BUCKET:-ruchoir}" /srv/migration/objects \
  --s3-provider Other --s3-endpoint "http://127.0.0.1:${GARAGE_S3_HOST_PORT:-3900}" \
  --s3-access-key-id "$S3_ACCESS_KEY_ID" --s3-secret-access-key "$S3_SECRET_ACCESS_KEY" \
  --s3-force-path-style --s3-region garage
# 4. The key that opens the secrets the monolith kept encrypted, in a file only you read.
umask 077; printf '%s' "$RUCHOIR_SECRET_ENCRYPTION_KEY" > /srv/migration/monolith.key
# 5. Stop the monolith (its volumes stay: they are the way back).
docker compose down
```

`rclone check` with the same options compares the copy with the bucket: run it once the copy ends.
Keep `/srv/migration` encrypted at rest or delete it when done: it holds everyone's data and a key.

Then the new instance, in its own directory (`/srv/ruchoir`):

```sh
# 6. Install it with every product the monolith had, and start it once: each service makes its tables.
ruchoir-install init --domain example.org --acme-email admin@example.org \
  --products drive,calendar,chat --dir /srv/ruchoir
ruchoir-install up --dir /srv/ruchoir
# Nobody signs up or creates anything now: the migration refuses an instance that holds accounts
# or organisations the monolith does not.

# 7. A dry run: everything read and written, then rolled back; the report says what would happen.
ruchoir-install migrate --dir /srv/ruchoir --from /srv/migration/monolith.dump \
  --objects /srv/migration/objects --monolith-key-file /srv/migration/monolith.key --dry-run

# 8. The migration. It stops the services, publishes PostgreSQL and Garage on the loopback for its
#    own use, restores the dump into a database of its own, carries everything over, copies the
#    objects, writes keys/hub-vapid.pem and migration-report.json, and starts the instance again.
ruchoir-install migrate --dir /srv/ruchoir --from /srv/migration/monolith.dump \
  --objects /srv/migration/objects --monolith-key-file /srv/migration/monolith.key
```

A run that stops half way (a full disk, a lost connection) is finished by running it again: every
row and every object already there is kept, and the report counts them as "kept".

The ports it publishes while it runs are `127.0.0.1:15432` (PostgreSQL) and `127.0.0.1:13900`
(Garage); `--postgres-port` and `--garage-port` change them.

## Checking

1. **The report.** "missing from the copy of the store" must be empty: a missing object is a file
   whose bytes the copy lacks (copy them, run again). "Left behind" must match the table above; a
   number there that you did not expect deserves a look before opening.
2. **Counts**, the monolith's against the new instance's:

   ```sh
   cd /srv/ruchoir
   q() { docker compose exec -T postgres psql -U ruchoir -d "$1" -Atc "$2"; }
   q monolith_source "SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM spaces), \
     (SELECT count(*) FROM files), (SELECT count(*) FROM messages), (SELECT count(*) FROM calendar_events)"
   q propolis "SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM organisations)"
   q drive "SELECT count(*) FROM files WHERE holder IS NULL"
   q chat "SELECT count(*) FROM messages"
   q calendar "SELECT count(*) FROM calendar_events"
   ```

   The drive holds more files than the monolith when messages or events carried files of the tree:
   each of those is now a copy the chat or the calendar holds (`holder` is set).
3. **By hand**, with a tester's account (or yours): sign in at `https://id.<domain>`, open each
   organisation, a folder and a document in the drive (preview and download), a channel and a
   direct conversation in the chat with their attachments, the calendar with an event and its
   files, the inbox, and an iCal subscription link. Sign in with two-factor on an account that
   has it. A passkey works only if the monolith's `RUCHOIR_WEBAUTHN_RP_ID` is `id.<domain>` or a
   domain it belongs to (`<domain>`); otherwise people sign in with their password and add the
   passkey again.
4. When everything holds: drop the restored copy, which duplicates everyone's data,
   `docker compose exec postgres dropdb -U ruchoir monolith_source`, and remove
   `compose.migrate.yaml`.

## Opening

Point the testers at the new addresses (the apps ask for `<domain>` at sign-in), and keep the
monolith's volumes and `/srv/migration` until the testers have used the new instance for a while.

## Going back

Nothing of the monolith was changed. Stop the new instance (`ruchoir-install down --dir /srv/ruchoir`)
and start the monolith again (`docker compose up -d` in its directory). What people did on the new
instance in between is not carried back. To try the migration again from scratch, remove the new
instance and its volumes (`docker compose down -v` in `/srv/ruchoir`, then the directory) and start at
step 6.

## When the monolith's project is `ruchoir`

The two instances must not share volumes. Before the day, with the monolith stopped and a backup
taken (its `scripts/backup.sh`), give its volumes another project name, `monolith`, by copying each:

```sh
for v in postgres-data valkey-data garage-meta garage-data; do
  docker volume create "monolith_$v"
  docker run --rm -v "ruchoir_$v:/from:ro" -v "monolith_$v:/to" alpine cp -a /from/. /to/
done
```

then start the monolith as `COMPOSE_PROJECT_NAME=monolith docker compose up -d` (and use that name
for every command of the monolith from then on, including going back), check it works, and only
then remove the `ruchoir_*` volumes (`docker volume rm ruchoir_postgres-data ...`). Volumes of the
monolith with other names (its `acme`, its office cache) follow the same way.
