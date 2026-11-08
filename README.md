# Ruchoir installer

The installer sets up a self-hosted Ruchoir instance à la carte. Propolis, the hub and the reverse
proxy are always installed, plus the products chosen (choosing the chat also installs the drive). It
keeps the manifest of installed products, published at a `.well-known` address.

> In construction: part of the restructuring of Ruchoir into separate products. Version 0 installs
> Propolis and the hub, and the drive when asked; the other products follow.

## Use

```sh
ruchoir-install init --domain example.org --acme-email admin@example.org --dir /srv/ruchoir
ruchoir-install up --dir /srv/ruchoir
```

`init` writes the instance's directory: the secrets (`.env`) and keys (`keys/`), kept when it runs
again, and everything else written anew. Propolis answers at `id.example.org`, the hub at
`hub.example.org`; the certificates come from Actalis by ACME. Your own changes (the SMTP relay,
for example) go in `compose.override.yaml`, which `init` never touches.

The drive comes with `--products drive`, with Garage to keep its files:

```sh
ruchoir-install init --domain example.org --acme-email admin@example.org --products drive --dir /srv/ruchoir
ruchoir-install up --dir /srv/ruchoir
```

It answers at `drive.example.org`. `up` also prepares Garage the first time (its layout, the drive's
key and bucket), and checks it again on every start. The products asked once are kept: a later
`init` without `--products` installs them still, and none is ever removed by `init`.

For development, with the `propolis` and `hub` repositories (and `drive`, with the drive) checked
out side by side:

```sh
NODE_AUTH_TOKEN=... ruchoir-install dev --source .. --products drive
```

builds their images, starts the instance at `https://id.localhost`, and catches every mail at
`http://localhost:8025`. Trust `ruchoir-dev/dev-ca/ca.pem` in your browser.

- How to work here: [`AGENTS.md`](AGENTS.md), which opens with the common rules of the organization.
- How to contribute: [Ruchoir/.github, `CONTRIBUTING.md`](https://github.com/Ruchoir/.github/blob/main/CONTRIBUTING.md).
- Decisions that span several repositories: [Ruchoir/.github, `docs/adr`](https://github.com/Ruchoir/.github/tree/main/docs/adr).

## Licence

`AGPL-3.0-only` (`LICENSE`).
