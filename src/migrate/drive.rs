//! The drive: the file tree, its versions and bytes, its rules, links and stars; and the files the
//! chat and the calendar now hold in it.
//!
//! | Monolith | Drive | How |
//! |---|---|---|
//! | `files` (in a space's tree) | `files` | `space_id` is `organisation_id`; the folder that public attachments landed in (`system_key`) stays an ordinary folder |
//! | `files` with a `conversation_id` | `files` held by the chat | `holder` `chat`, `holder_ref` `conversation:<id>`, out of the tree; left behind without the chat |
//! | `file_versions` | `file_versions` | as they are, object keys included (`spaces/...`): the drive reads a version's key from its row |
//! | `file_stars` | `file_stars` | `user_id` is `account_id` |
//! | `file_links` | `file_links` | `space_id` is `organisation_id` |
//! | `permission_overrides` on a file | `file_rules` | a role, a group or a person; the drive's permissions only; a rule for "the readers of a channel" is left behind (no product answers for another's audience) |
//! | `file_shares` | left behind | the monolith had stopped reading them (they grant nothing) |
//! | the object store | the drive's bucket | each version's object, its thumbnail (`.thumb`) and its PDF rendering (`.pdf`), under the same key |
//!
//! **The files a product holds.** The chat records a message's file as a file it holds for the
//! conversation, one message per file; the calendar, an event's file as a file it holds for the
//! event, one event per file. The monolith linked any file of the tree instead. So each file a
//! message carries becomes the chat's: the conversation's own file when it is the first message to
//! carry it, otherwise a copy (its current version, object copied under the drive's own key
//! scheme). Each file an event carries becomes a copy held by the calendar. A copy's ids are
//! derived from the source's ([`crate::migrate::derived`]), so a second run finds them.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::migrate::db::{self, Source};
use crate::migrate::replica::{self, Model, DRIVE};
use crate::migrate::report::Report;
use crate::migrate::{derived, id, text, Migration};

/// A file a product holds, as the product records it.
#[derive(Debug, Clone)]
pub struct Held {
    pub file_id: String,
    pub organisation_id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: i64,
}

/// The files the chat and the calendar hold, by what carries them.
#[derive(Debug, Default)]
pub struct HeldFiles {
    /// `(message, monolith file)` to the file the chat holds.
    pub messages: BTreeMap<(String, String), Held>,
    /// `(event, monolith file)` to the file the calendar holds.
    pub events: BTreeMap<(String, String), Held>,
}

const FILES: &str = "WITH RECURSIVE tree AS ( \
    SELECT id, 0 AS depth FROM files WHERE parent_folder_id IS NULL \
    UNION ALL SELECT f.id, t.depth + 1 FROM files f JOIN tree t ON f.parent_folder_id = t.id) \
    SELECT f.id, f.space_id AS organisation_id, f.owner_id, f.name, f.kind, f.parent_folder_id, \
    f.current_version_id, f.size_bytes, f.imported_source, f.external_ref, f.created_at, \
    f.updated_at, f.deleted_at, f.deleted_by, f.trashed, f.purged_at, f.conversation_id \
    FROM files f JOIN tree t ON t.id = f.id ORDER BY t.depth, f.created_at, f.id";

const VERSIONS: &str = "SELECT id, file_id, version_no, size_bytes, content_hash, storage_key, \
    thumbnail_key, mime_type, image_width, image_height, created_by, created_at FROM file_versions";

/// A copy of `file` (and of its current version) held by `holder` for `holder_ref`, or `None`
/// when it has no bytes left to copy.
fn copy_of(
    holder: &str,
    holder_ref: &str,
    carrier: &str,
    file: &Value,
    version: Option<&Value>,
    organisation: &str,
) -> Option<(Value, Value, Held)> {
    let version = version?;
    if !file["purged_at"].is_null() || version["storage_key"].is_null() {
        return None;
    }
    let source_file = text(file, "id");
    let file_id = derived(&[holder, carrier, source_file]).to_string();
    let version_id = derived(&[holder, carrier, source_file, text(version, "id")]).to_string();
    let key = format!("organisations/{organisation}/{file_id}/{version_id}");
    let mut row = file.clone();
    let fields = row.as_object_mut()?;
    fields.remove("conversation_id");
    fields.insert("id".into(), json!(file_id));
    fields.insert("organisation_id".into(), json!(organisation));
    fields.insert("parent_folder_id".into(), Value::Null);
    fields.insert("current_version_id".into(), json!(version_id));
    fields.insert("size_bytes".into(), version["size_bytes"].clone());
    fields.insert("deleted_at".into(), Value::Null);
    fields.insert("deleted_by".into(), Value::Null);
    fields.insert("trashed".into(), json!(false));
    fields.insert("holder".into(), json!(holder));
    fields.insert("holder_ref".into(), json!(holder_ref));
    let mut copy = version.clone();
    copy["id"] = json!(version_id);
    copy["file_id"] = json!(file_id);
    copy["version_no"] = json!(1);
    // The objects are copied from the source's keys ([`objects`]); the row names the new ones.
    copy["source_storage_key"] = version["storage_key"].clone();
    copy["source_thumbnail_key"] = version["thumbnail_key"].clone();
    copy["storage_key"] = json!(key);
    if !version["thumbnail_key"].is_null() {
        copy["thumbnail_key"] = json!(format!("{key}.thumb"));
    }
    let held = Held {
        file_id,
        organisation_id: organisation.to_owned(),
        name: text(file, "name").to_owned(),
        mime_type: text(version, "mime_type").to_owned(),
        size_bytes: version["size_bytes"].as_i64().unwrap_or(0),
    };
    Some((row, copy, held))
}

/// Where the objects of the versions carried over come from and go to.
fn object_moves(versions: &[Value]) -> Vec<(String, String)> {
    let mut moves = Vec::new();
    for v in versions {
        let from = v
            .get("source_storage_key")
            .filter(|k| !k.is_null())
            .unwrap_or(&v["storage_key"]);
        if let (Some(from), Some(to)) = (from.as_str(), v["storage_key"].as_str()) {
            moves.push((from.to_owned(), to.to_owned()));
            // A rendering kept beside a document, when the copy has one.
            moves.push((format!("{from}.pdf"), format!("{to}.pdf")));
        }
        let from = v
            .get("source_thumbnail_key")
            .filter(|k| !k.is_null())
            .unwrap_or(&v["thumbnail_key"]);
        if let (Some(from), Some(to)) = (from.as_str(), v["thumbnail_key"].as_str()) {
            moves.push((from.to_owned(), to.to_owned()));
        }
    }
    moves
}

pub async fn run(
    migration: &Migration<'_>,
    source: &Source<'_>,
    model: &Model,
    report: &mut Report,
) -> Result<HeldFiles, String> {
    let mut held = HeldFiles::default();
    let Some(target) = &migration.targets.drive else {
        let files = db::count(migration.source, "SELECT count(*) FROM files").await?;
        report.drop("files (the drive is not installed)", files as u64);
        return Ok(held);
    };
    let with_chat = migration.targets.chat.is_some();
    let with_calendar = migration.targets.calendar.is_some();
    db::begin(target).await?;
    replica::write(target, "drive", DRIVE, model, report).await?;

    let files = source.rows(FILES).await?;
    let versions = source.rows(VERSIONS).await?;
    let version_of: BTreeMap<&str, &Value> = versions.iter().map(|v| (text(v, "id"), v)).collect();
    let file_of: BTreeMap<&str, &Value> = files.iter().map(|f| (text(f, "id"), f)).collect();

    // The tree, and the conversations' files the chat now holds.
    let mut rows = Vec::new();
    let mut kept_files = BTreeSet::new();
    let mut without_chat = 0;
    for file in &files {
        let mut row = file.clone();
        let conversation = file["conversation_id"].as_str().map(str::to_owned);
        let fields = row.as_object_mut().ok_or("a file is not a row")?;
        fields.remove("conversation_id");
        match conversation {
            Some(conversation) if with_chat => {
                fields.insert("holder".into(), json!("chat"));
                fields.insert(
                    "holder_ref".into(),
                    json!(format!("conversation:{conversation}")),
                );
                fields.insert("parent_folder_id".into(), Value::Null);
            }
            Some(_) => {
                without_chat += 1;
                continue;
            }
            None => {}
        }
        kept_files.insert(text(file, "id").to_owned());
        rows.push(row);
    }
    report.drop(
        "conversation files (the chat is not installed)",
        without_chat,
    );
    let mut carried: Vec<Value> = versions
        .iter()
        .filter(|v| kept_files.contains(text(v, "file_id")))
        .cloned()
        .collect();

    // A message's files: the conversation's own file for the first message, a copy otherwise.
    let mut copies = Vec::new();
    let mut copied_versions = Vec::new();
    let mut gone = 0;
    if with_chat {
        let attachments = source
            .rows(
                "SELECT a.message_id, a.file_id, m.conversation_id, c.space_id \
                 FROM message_attachments a JOIN messages m ON m.id = a.message_id \
                 JOIN conversations c ON c.id = m.conversation_id \
                 ORDER BY m.created_at, a.message_id, a.position",
            )
            .await?;
        let mut used = BTreeSet::new();
        for a in &attachments {
            let (message, file_id) = (text(a, "message_id"), text(a, "file_id"));
            let Some(file) = file_of.get(file_id) else {
                gone += 1;
                continue;
            };
            let version = file["current_version_id"]
                .as_str()
                .and_then(|v| version_of.get(v).copied());
            let own = file["conversation_id"].as_str() == Some(text(a, "conversation_id"));
            if own && used.insert(file_id.to_owned()) {
                held.messages.insert(
                    (message.to_owned(), file_id.to_owned()),
                    Held {
                        file_id: file_id.to_owned(),
                        organisation_id: text(file, "organisation_id").to_owned(),
                        name: text(file, "name").to_owned(),
                        mime_type: version
                            .map(|v| text(v, "mime_type"))
                            .unwrap_or("")
                            .to_owned(),
                        size_bytes: file["size_bytes"].as_i64().unwrap_or(0),
                    },
                );
                continue;
            }
            let holder_ref = format!("conversation:{}", text(a, "conversation_id"));
            match copy_of(
                "chat",
                &holder_ref,
                message,
                file,
                version,
                text(a, "space_id"),
            ) {
                Some((row, version, file_held)) => {
                    copies.push(row);
                    copied_versions.push(version);
                    held.messages
                        .insert((message.to_owned(), file_id.to_owned()), file_held);
                }
                None => gone += 1,
            }
        }
    }

    // An event's files: a copy each, held by the calendar.
    if with_calendar {
        let event_files = source
            .rows(
                "SELECT ef.event_id, ef.file_id, c.space_id FROM calendar_event_files ef \
                 JOIN calendar_events e ON e.id = ef.event_id JOIN calendars c ON c.id = e.calendar_id",
            )
            .await?;
        for ef in &event_files {
            let (event, file_id) = (text(ef, "event_id"), text(ef, "file_id"));
            let Some(file) = file_of.get(file_id) else {
                gone += 1;
                continue;
            };
            let version = file["current_version_id"]
                .as_str()
                .and_then(|v| version_of.get(v).copied());
            // A personal calendar has no organisation: the file stays in its own.
            let organisation = ef["space_id"]
                .as_str()
                .unwrap_or_else(|| text(file, "organisation_id"));
            let holder_ref = format!("event:{event}");
            match copy_of("calendar", &holder_ref, event, file, version, organisation) {
                Some((row, version, file_held)) => {
                    copies.push(row);
                    copied_versions.push(version);
                    held.events
                        .insert((event.to_owned(), file_id.to_owned()), file_held);
                }
                None => gone += 1,
            }
        }
    }
    report.drop(
        "attached files whose bytes are gone (erased in the monolith)",
        gone,
    );

    rows.extend(copies);
    db::insert(target, "files", &rows, report, "drive.files").await?;
    let mut moves = object_moves(&carried);
    moves.extend(object_moves(&copied_versions));
    for v in &mut copied_versions {
        if let Some(fields) = v.as_object_mut() {
            fields.remove("source_storage_key");
            fields.remove("source_thumbnail_key");
        }
    }
    carried.extend(copied_versions);
    db::insert(
        target,
        "file_versions",
        &carried,
        report,
        "drive.file_versions",
    )
    .await?;

    let stars = source
        .rows("SELECT user_id AS account_id, file_id, created_at FROM file_stars")
        .await?;
    let stars: Vec<Value> = stars
        .into_iter()
        .filter(|s| kept_files.contains(text(s, "file_id")))
        .collect();
    db::insert(target, "file_stars", &stars, report, "drive.file_stars").await?;
    let links = source
        .rows(
            "SELECT id, file_id, space_id AS organisation_id, token, created_by, created_at, \
             expires_at, password_hash, download_count, revoked_at FROM file_links",
        )
        .await?;
    let links: Vec<Value> = links
        .into_iter()
        .filter(|l| kept_files.contains(text(l, "file_id")))
        .collect();
    db::insert(target, "file_links", &links, report, "drive.file_links").await?;

    let rules = rules(source, "file_id", DRIVE, report).await?;
    db::insert(target, "file_rules", &rules, report, "drive.file_rules").await?;
    let shares = db::count(migration.source, "SELECT count(*) FROM file_shares").await?;
    report.drop("legacy file shares (they granted nothing)", shares as u64);

    db::finish(target, migration.dry_run).await?;
    copy_objects(migration, &moves, report).await?;
    Ok(held)
}

/// The monolith's rules on the resources of `column` (`file_id`, `calendar_id`, `channel_id`),
/// as the rules of a product whose permissions are `own`: a role, a group or a person.
pub async fn rules(
    source: &Source<'_>,
    column: &str,
    own: &[&str],
    report: &mut Report,
) -> Result<Vec<Value>, String> {
    let rows = source
        .rows(&format!(
            "SELECT id, space_id AS organisation_id, {column} AS resource_id, role_id, group_id, \
             user_id, audience_channel_id, allow, deny, coalesce(set_by, created_by) AS created_by, \
             created_at, updated_at FROM permission_overrides WHERE {column} IS NOT NULL"
        ))
        .await?;
    let mut rules = Vec::new();
    let mut audience = 0;
    for r in rows {
        let (kind, subject) = if let Some(role) = id(&r, "role_id") {
            ("role", role)
        } else if let Some(group) = id(&r, "group_id") {
            ("group", group)
        } else if let Some(person) = id(&r, "user_id") {
            ("person", person)
        } else {
            audience += 1;
            continue;
        };
        let mut rule = json!({
            "id": r["id"],
            "organisation_id": r["organisation_id"],
            column: r["resource_id"],
            "subject_kind": kind,
            "subject_id": subject.to_string(),
            "allow": replica::kept(&r["allow"], own),
            "deny": replica::kept(&r["deny"], own),
            "created_by": r["created_by"],
            "created_at": r["created_at"],
        });
        if column != "file_id" {
            rule["updated_at"] = r["updated_at"].clone();
        }
        rules.push(rule);
    }
    report.drop("rules for the readers of a channel", audience);
    Ok(rules)
}

/// Copy each object from the copy of the monolith's store to the drive's, skipping what is there.
async fn copy_objects(
    migration: &Migration<'_>,
    moves: &[(String, String)],
    report: &mut Report,
) -> Result<(), String> {
    for (from, to) in moves {
        let optional = from.ends_with(".pdf");
        let Some(bytes) = migration.objects.get(from) else {
            if !optional {
                report.objects.read += 1;
                report.objects_missing.push(from.clone());
            }
            continue;
        };
        report.objects.read += 1;
        let Some(store) = migration.store else {
            continue;
        };
        if store.has(to).await? {
            report.objects.kept += 1;
            continue;
        }
        store.put(to, &bytes).await?;
        report.objects.written += 1;
    }
    Ok(())
}
