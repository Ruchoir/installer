//! The chat.
//!
//! | Monolith | Chat | How |
//! |---|---|---|
//! | `conversations`, `dm_conversations`, `dm_participants` | the same | `space_id` is `organisation_id` |
//! | `channels` | `channels` | `is_default` where the monolith kept `spaces.default_channel_id`; `imported_source` and `external_ref` are left behind |
//! | `channel_role_access` | left behind | the monolith kept it mirrored in its rules, which come |
//! | `permission_overrides` on a channel | `chat_rules` | a role, a group or a person; the chat's permissions only |
//! | `channel_members`, `channel_pins`, `read_cursors`, `message_mentions`, `message_reactions` | the same | as they are |
//! | `messages` | `messages` | as they are (the search index is rebuilt by PostgreSQL); a calendar card's `event_id` is left behind |
//! | `user_saved_messages` | `saved_messages` | as they are |
//! | `message_attachments` | `message_attachments` | the file the chat now holds ([`super::drive`]), with its name, type and size |
//! | `message_link_previews` | `message_link_previews` | the first preview of a message, its picture read from the object store (256 KiB at most) |
//! | `notifications` about a message | `chat_notices` | who was told what about which message |

use serde_json::{json, Value};

use crate::migrate::crypto;
use crate::migrate::db::{self, Source};
use crate::migrate::drive::{self as drive_part, HeldFiles};
use crate::migrate::replica::{self, Model, CHAT};
use crate::migrate::report::Report;
use crate::migrate::{text, Migration};

/// The largest picture of a link preview the chat keeps (its `CHECK`).
const PREVIEW_IMAGE_MAX: usize = 262_144;

pub async fn run(
    migration: &Migration<'_>,
    source: &Source<'_>,
    model: &Model,
    held: &HeldFiles,
    report: &mut Report,
) -> Result<(), String> {
    let Some(target) = &migration.targets.chat else {
        let messages = db::count(migration.source, "SELECT count(*) FROM messages").await?;
        report.drop("messages (the chat is not installed)", messages as u64);
        return Ok(());
    };
    db::begin(target).await?;
    replica::write(target, "chat", CHAT, model, report).await?;

    let steps = [
        (
            "conversations",
            "SELECT id, space_id AS organisation_id, kind, created_at FROM conversations",
        ),
        (
            "channels",
            "SELECT c.id, c.space_id AS organisation_id, c.name::text AS name, c.type, c.topic, \
             c.created_by, c.archived_at, c.position, \
             (s.default_channel_id IS NOT DISTINCT FROM c.id) AS is_default, c.created_at \
             FROM channels c JOIN spaces s ON s.id = c.space_id",
        ),
        (
            "dm_conversations",
            "SELECT id, space_id AS organisation_id, is_group, created_by, created_at FROM dm_conversations",
        ),
        (
            "dm_participants",
            "SELECT dm_id, user_id, notification_level, muted, hidden, added_at FROM dm_participants",
        ),
        ("channel_members", "SELECT * FROM channel_members"),
        (
            "messages",
            "SELECT id, conversation_id, author_id, kind, body, system_event, parent_message_id, \
             also_in_channel, reply_count, created_at, edited_at, deleted_at FROM messages \
             ORDER BY parent_message_id IS NOT NULL, created_at, id",
        ),
        ("channel_pins", "SELECT * FROM channel_pins"),
        ("read_cursors", "SELECT * FROM read_cursors"),
        ("message_mentions", "SELECT * FROM message_mentions"),
        ("message_reactions", "SELECT * FROM message_reactions"),
        ("saved_messages", "SELECT user_id, message_id, saved_at FROM user_saved_messages"),
        (
            "chat_notices",
            "SELECT id, user_id AS account_id, message_id, kind, actor_id, created_at \
             FROM notifications WHERE message_id IS NOT NULL AND kind NOT LIKE 'calendar\\_%'",
        ),
    ];
    for (table, query) in steps {
        let rows = source.rows(query).await?;
        db::insert(target, table, &rows, report, &format!("chat.{table}")).await?;
    }
    let rules = drive_part::rules(source, "channel_id", CHAT, report).await?;
    db::insert(target, "chat_rules", &rules, report, "chat.chat_rules").await?;

    let mut attachments = Vec::new();
    for a in source
        .rows(
            "SELECT a.message_id, a.file_id, a.position, a.alt_text, m.created_at AS added_at \
             FROM message_attachments a JOIN messages m ON m.id = a.message_id",
        )
        .await?
    {
        let carrier = (
            text(&a, "message_id").to_owned(),
            text(&a, "file_id").to_owned(),
        );
        if let Some(file) = held.messages.get(&carrier) {
            attachments.push(json!({
                "message_id": a["message_id"],
                "file_id": file.file_id,
                "organisation_id": file.organisation_id,
                "position": a["position"],
                "alt_text": a["alt_text"],
                "name": file.name,
                "mime_type": if file.mime_type.is_empty() { "application/octet-stream" } else { &file.mime_type },
                "size_bytes": file.size_bytes,
                "added_at": a["added_at"],
            }));
        }
    }
    db::insert(
        target,
        "message_attachments",
        &attachments,
        report,
        "chat.message_attachments",
    )
    .await?;

    let previews = source
        .rows(
            "SELECT DISTINCT ON (message_id) id, message_id, url, domain, title, description, color, \
             image_key, image_width, image_height, fetched_at FROM message_link_previews \
             ORDER BY message_id, fetched_at, id",
        )
        .await?;
    let mut rows = Vec::new();
    for mut p in previews {
        let image = p["image_key"]
            .as_str()
            .and_then(|key| migration.objects.get(key))
            .filter(|bytes| bytes.len() <= PREVIEW_IMAGE_MAX);
        let fields = p.as_object_mut().ok_or("a preview is not a row")?;
        fields.remove("image_key");
        match image {
            Some(bytes) => {
                fields.insert("image".into(), Value::from(crypto::to_bytea(&bytes)));
            }
            None => {
                fields.insert("image".into(), Value::Null);
                fields.insert("image_width".into(), Value::Null);
                fields.insert("image_height".into(), Value::Null);
            }
        }
        rows.push(p);
    }
    db::insert(
        target,
        "message_link_previews",
        &rows,
        report,
        "chat.message_link_previews",
    )
    .await?;
    let extra = db::count(
        migration.source,
        "SELECT count(*) - count(DISTINCT message_id) FROM message_link_previews",
    )
    .await?;
    report.drop("link previews beyond a message's first", extra as u64);
    db::finish(target, migration.dry_run).await
}
