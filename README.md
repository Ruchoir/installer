# Ruchoir installer

The installer sets up a self-hosted Ruchoir instance à la carte. Propolis, the hub and the reverse
proxy are always installed, plus the products chosen (choosing the chat also installs the drive). It
keeps the manifest of installed products, published at a `.well-known` address.

> In construction: part of the restructuring of Ruchoir into separate products. Nothing runs here yet.

- How to work here: [`AGENTS.md`](AGENTS.md), which opens with the common rules of the organization.
- How to contribute: [Ruchoir/.github, `CONTRIBUTING.md`](https://github.com/Ruchoir/.github/blob/main/CONTRIBUTING.md).
- Decisions that span several repositories: [Ruchoir/.github, `docs/adr`](https://github.com/Ruchoir/.github/tree/main/docs/adr).

## Licence

`AGPL-3.0-only` (`LICENSE`).
