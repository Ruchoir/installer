# 0001. Migrating an instance of the monolith

- Status: accepted
- Date: 2026-11-13

## Context

The testers' instance runs the former single application (`Ruchoir/ruchoir`), and its data must be
kept when it moves to the services. The monolith's database is one schema for everything; each
service now has its own database, its own tables (mostly the monolith's, renamed and split) and, for
the products, a copy of each organisation that it normally reads from Propolis's change feed. The
monolith's importer reads other products' exports (Nextcloud, Mattermost, Slack, Teams) into the
monolith's own tables: it goes the other way and does not fit.

## Decision

- **The installer migrates**, as `ruchoir-install migrate`: it knows the instance's directory, its
  secrets and every service's database, and it is what an operator already runs. The code is in
  `src/migrate/`, one module per service, each with its table of what goes where.
- **Offline, from copies**: a dump of the monolith (`pg_dump --format=custom`) and a copy of its
  object store (a directory, each object at its key), into an instance that has started once and
  holds nothing else. The dump is restored into a database of the instance's PostgreSQL, published
  on the loopback for the time of the run, as Garage is.
- **Straight into the services' tables**, not through their APIs: no service has an import route,
  and none is added for a one-time move. The ids are kept, so every reference stays true; the
  products' copies of the organisations are written as the products would have read them.
- **Idempotent**: every write keeps what is there, derived ids are UUID v5 of the source's, the
  copies of the organisations are replaced whole. A dry run writes everything in transactions it
  rolls back.
- **The tests** apply schemas taken from the monolith's own migrations and from each service's
  (`tests/fixtures/`), so the installer still builds and tests alone. They must be taken again when a
  service's migrations change what the migration writes.

## Consequences

- The migration depends on the services' schemas without their code: a service changing a table
  the migration writes breaks it, and its fixture shows where.
- It runs once per instance. Once the testers' instance has moved, it stays for whoever ran the
  monolith, until no such instance is left.
