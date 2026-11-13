//! `ruchoir-install migrate`: carry the data of an instance of the former single application (the
//! monolith, `Ruchoir/ruchoir`) over to an instance of the services, once, before it is opened.
//!
//! The source is a copy: the monolith's database restored next to the instance's (`pg_dump` in the
//! custom format, see [`instance`]) and a copy of its object store, a directory where each object
//! sits at its key. Nothing of the monolith is ever written.
//!
//! What goes where, table by table, and what is left behind and why, is said in each module:
//! [`identity`] (Propolis: accounts, organisations, memberships, roles, groups, invitations,
//! journal), [`hub`] (inbox, push, notification settings, presence), [`drive`] (files, folders,
//! versions, rules, links, stars, blobs), [`calendar`] and [`chat`]. [`replica`] writes the copy of
//! each organisation every product keeps, as it would have read it from Propolis.
//!
//! **Ids are kept.** An account, an organisation, a file, a message keeps the id it had, so every
//! reference between them stays true and a link to a file or a message still leads to it. The
//! few rows the new layout needs and the monolith never had (a file a product now holds in the
//! drive for a message or an event) get an id derived from the source's (UUID v5), the same on
//! every run.
//!
//! **Run again, it does the same.** Every write is `ON CONFLICT DO NOTHING` on those ids, and the
//! replicas, which are copies, are replaced whole. A run that stopped half way is finished by the
//! next one. It refuses a target that holds accounts or organisations the source does not: it
//! fills a fresh instance, it never merges two.
//!
//! **A dry run** reads everything and writes everything inside transactions it then rolls back,
//! and copies no object: the report says what a real run would do.

pub mod calendar;
pub mod chat;
pub mod crypto;
pub mod db;
pub mod drive;
pub mod hub;
pub mod identity;
pub mod instance;
pub mod objects;
pub mod replica;
pub mod report;

use std::collections::BTreeMap;

use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

use crate::migrate::objects::{Objects, Store};
use crate::migrate::report::Report;

/// The namespace of the ids this migration derives (UUID v5), fixed for good: changing it would
/// make a second run write every derived row again.
pub const NAMESPACE: Uuid = Uuid::from_u128(0x6f2c_1d0e_8a4b_4c3e_9b1f_52d7_a0c4_e911);

/// An id derived from `parts`, the same on every run.
pub fn derived(parts: &[&str]) -> Uuid {
    Uuid::new_v5(&NAMESPACE, parts.join("/").as_bytes())
}

/// The databases written: Propolis's and the hub's always, a product's when it is installed.
pub struct Targets {
    pub propolis: Client,
    pub hub: Client,
    pub drive: Option<Client>,
    pub calendar: Option<Client>,
    pub chat: Option<Client>,
}

/// The keys secrets at rest are kept under.
pub struct Keys {
    /// The monolith's `RUCHOIR_SECRET_ENCRYPTION_KEY`.
    pub monolith: [u8; 32],
    /// The instance's `PROPOLIS_SECRET_KEY`.
    pub propolis: [u8; 32],
    /// The instance's `CALENDAR_SECRET_KEY`, with the calendar.
    pub calendar: Option<[u8; 32]>,
}

/// The public addresses of the instance's services (`https://chat.<domain>`), from its manifest.
pub type Origins = BTreeMap<String, String>;

/// Everything a run needs.
pub struct Migration<'a> {
    pub source: &'a Client,
    pub targets: &'a Targets,
    pub keys: &'a Keys,
    pub origins: &'a Origins,
    /// The copy of the monolith's object store.
    pub objects: &'a Objects,
    /// The drive's object store; `None` in a dry run, and without the drive.
    pub store: Option<&'a Store>,
    pub dry_run: bool,
}

/// What a run gives back besides its report: the VAPID key the hub should sign with, when the
/// monolith had one (the caller writes it to the instance's `keys/hub-vapid.pem`).
pub struct Outcome {
    pub report: Report,
    pub vapid_pem: Option<String>,
}

/// Carry everything over. Each database is written in one transaction, committed at the end of its
/// part (rolled back in a dry run); the parts run in the order their references need.
pub async fn run(migration: &Migration<'_>) -> Result<Outcome, String> {
    let mut report = Report::default();
    identity::guard(migration).await?;
    let source = db::Source::new(migration.source);
    let model = replica::Model::read(&source).await?;

    identity::run(migration, &source, &model, &mut report).await?;
    // The files first: the chat's attachments and the calendar's files are files of the drive.
    let held = drive::run(migration, &source, &model, &mut report).await?;
    calendar::run(migration, &source, &model, &held, &mut report).await?;
    chat::run(migration, &source, &model, &held, &mut report).await?;
    let vapid_pem = hub::run(migration, &source, &model, &mut report).await?;
    report.dry_run = migration.dry_run;
    Ok(Outcome { report, vapid_pem })
}

/// A JSON object's field as a string, or the empty string.
pub(crate) fn text<'v>(row: &'v Value, field: &str) -> &'v str {
    row.get(field).and_then(Value::as_str).unwrap_or("")
}

/// A JSON object's field as an id.
pub(crate) fn id(row: &Value, field: &str) -> Option<Uuid> {
    row.get(field)
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
}
