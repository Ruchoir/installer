# Ruchoir installer

The installer sets up a self-hosted Ruchoir instance à la carte. Propolis, the hub and the reverse
proxy are always installed, plus the products chosen (choosing the chat also installs the drive). It
keeps the manifest of installed products, published at a `.well-known` address.

> In construction: part of the restructuring of Ruchoir into separate products. Version 0 installs
> Propolis and the hub, and the drive, the calendar and the chat when asked; the other products
> follow.

## Use

```sh
ruchoir-install init --domain example.org --acme-email admin@example.org --dir /srv/ruchoir
ruchoir-install up --dir /srv/ruchoir
```

`init` writes the instance's directory: the secrets (`.env`) and keys (`keys/`), kept when it runs
again, and everything else written anew. Propolis answers at `id.example.org`, the hub at
`hub.example.org`; the certificates come from Actalis by ACME. Your own changes (the SMTP relay,
for example) go in `compose.override.yaml`, which `init` never touches.

The drive comes with `--products drive`, with Garage to keep its files and Euro-Office to edit
office documents live:

```sh
ruchoir-install init --domain example.org --acme-email admin@example.org --products drive --dir /srv/ruchoir
ruchoir-install up --dir /srv/ruchoir
```

It answers at `drive.example.org`, and its office editor at `office.example.org`: point both at the
machine. The editor's engine is reached through the drive alone and has no route out; it takes about
2 GB of memory (4 GB at most, `OFFICE_MEM_LIMIT` in `.env` to change it), 7 GB of disk for its
image, and a minute or two to start. `up` also prepares Garage the first time (its layout, the
drive's key and bucket), and checks it again on every start. The products asked once are kept: a later
`init` without `--products` installs them still, and none is ever removed by `init`.

The calendar comes with `--products calendar`, on its own or beside the drive
(`--products drive,calendar`): it answers at `calendar.example.org`. With the drive, events carry
files the drive keeps. The calendar mails the people invited by address; the installer sets no mail
relay in production, so until you give it yours in `compose.override.yaml` (`CALENDAR_SMTP_HOST`,
`CALENDAR_SMTP_PORT`, `CALENDAR_SMTP_USERNAME`, `CALENDAR_SMTP_PASSWORD`, `CALENDAR_SMTP_FROM`, as
for Propolis), those mails are only logged.

The chat comes with `--products chat`, and always brings the drive, where messages keep their files:
it answers at `chat.example.org`. Beside the calendar (`--products chat,calendar`), events are posted
as cards in channels. The chat sends no mail; it reads the first link of a message for its preview,
never on the instance's own domain.

For development, with the `propolis` and `hub` repositories (and those of the products asked,
`drive`, `calendar`, `chat`) checked out side by side:

```sh
NODE_AUTH_TOKEN=... ruchoir-install dev --source .. --products calendar,chat
```

builds their images, starts the instance at `https://id.localhost`, and catches every mail at
`http://localhost:8025`. Trust `ruchoir-dev/dev-ca/ca.pem` in your browser.

- How to work here: [`AGENTS.md`](AGENTS.md), which opens with the common rules of the organization.
- How to contribute: [Ruchoir/.github, `CONTRIBUTING.md`](https://github.com/Ruchoir/.github/blob/main/CONTRIBUTING.md).
- Decisions that span several repositories: [Ruchoir/.github, `docs/adr`](https://github.com/Ruchoir/.github/tree/main/docs/adr).

## Licence

`AGPL-3.0-only` (`LICENSE`).
