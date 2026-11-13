//! The hub: the inbox, push, notification settings and presence.
//!
//! | Monolith | Hub | How |
//! |---|---|---|
//! | `space_members` | `organisation_members` | the hub's copy of who belongs where, as it reads it from Propolis |
//! | `users.manual_presence` | `presence_choices` | as it is |
//! | `push_devices` | `push_devices` | `user_id` is `account_id`; the relay's tokens do not depend on the instance |
//! | `push_subscriptions` | `push_subscriptions` | kept with the monolith's VAPID key, which the hub then signs with (`keys/hub-vapid.pem`); without that key, left behind |
//! | `user_preferences.notifications` | `notification_settings` | reshaped around the inbox's categories (below) |
//! | `space_notification_prefs` | `organisation_levels` | as they are |
//! | `notifications` about a message | `inbox_entries` | `chat.<kind>`, the chat's category, a link to the message, and the words the chat would have written (the place, the author, an excerpt) |
//! | `notifications` of the calendar | left behind | reminders that have passed, and invitations each event still shows |
//!
//! The rest of `user_preferences` (theme, font, text size, the pages' state) has no place on a
//! server any more: each product's pages keep their own.

use serde_json::{json, Map, Value};

use crate::migrate::crypto;
use crate::migrate::db::{self, Source};
use crate::migrate::replica::Model;
use crate::migrate::report::Report;
use crate::migrate::{text, Migration};

/// The longest excerpt of a message an entry shows, as the chat cuts it.
const EXCERPT_CHARS: usize = 140;

/// The hub's settings from the monolith's (`NotifPrefs` of its web app, camelCase): the switches
/// of each kind become those of the inbox's categories.
pub fn settings(old: &Value) -> Value {
    let on = |key: &str, default: bool| old.get(key).and_then(Value::as_bool).unwrap_or(default);
    let time = |key: &str, default: &str| {
        old.get(key)
            .and_then(Value::as_str)
            .filter(|t| t.len() == 5 && t.as_bytes()[2] == b':')
            .unwrap_or(default)
            .to_owned()
    };
    json!({
        "enabled": on("enabled", true),
        "sound": on("sound", true),
        "app": {
            "mention": on("mentions", true) || on("channelMentions", true),
            "reply": on("replies", true),
            "direct": on("directMessages", true),
            "activity": on("reactions", true) || on("edits", true),
            "reminder": on("calendarReminders", true),
            "invitation": on("calendarInvitations", true),
            "notice": true,
        },
        "email": on("email", true),
        "email_categories": {
            "mention": on("emailMentions", true) || on("emailBroadcasts", false),
            "reply": on("emailReplies", true),
            "direct": on("emailDirectMessages", true),
            "activity": false,
            "reminder": on("emailCalendarReminders", true),
            "invitation": on("emailCalendarInvitations", true),
            "notice": true,
        },
        "quiet_hours": on("quietHours", false),
        "quiet_from": time("quietFrom", "21:00"),
        "quiet_to": time("quietTo", "08:00"),
        // The pages send the offset of their clock on their next load.
        "utc_offset_minutes": 0,
    })
}

/// The inbox's category of a chat notification, as the chat files it.
pub fn category(kind: &str) -> &'static str {
    match kind {
        "mention" | "broadcast" => "mention",
        "reply" => "reply",
        "dm" => "direct",
        _ => "activity",
    }
}

/// A message's first words, as the chat quotes it in an entry.
pub fn excerpt(body: &str) -> String {
    let plain: String = body
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '~'))
        .collect();
    let flat = plain
        .split_whitespace()
        .filter(|word| *word != ">")
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= EXCERPT_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(EXCERPT_CHARS).collect();
    format!("{}\u{2026}", cut.trim_end())
}

fn language(locale: &str) -> &'static str {
    match locale.split(['-', '_']).next().unwrap_or("") {
        "en" => "en",
        "es" => "es",
        "de" => "de",
        "it" => "it",
        "pl" => "pl",
        _ => "fr",
    }
}

fn someone(language: &str) -> &'static str {
    match language {
        "en" => "Someone",
        "es" => "Alguien",
        "de" => "Jemand",
        "it" => "Qualcuno",
        "pl" => "Ktoś",
        _ => "Quelqu'un",
    }
}

fn did(kind: &str, language: &str) -> &'static str {
    match (kind, language) {
        ("reaction", "en") => "reacted to your message",
        ("reaction", "es") => "reaccionó a su mensaje",
        ("reaction", "de") => "hat auf Ihre Nachricht reagiert",
        ("reaction", "it") => "ha reagito al tuo messaggio",
        ("reaction", "pl") => "zareagował(a) na Twoją wiadomość",
        ("reaction", _) => "a réagi à votre message",
        ("edited", "en") => "edited a message",
        ("edited", "es") => "editó un mensaje",
        ("edited", "de") => "hat eine Nachricht bearbeitet",
        ("edited", "it") => "ha modificato un messaggio",
        ("edited", "pl") => "edytował(a) wiadomość",
        ("edited", _) => "a modifié un message",
        (_, "en") => "sent a message",
        (_, "es") => "envió un mensaje",
        (_, "de") => "hat eine Nachricht gesendet",
        (_, "it") => "ha inviato un messaggio",
        (_, "pl") => "wysłał(a) wiadomość",
        _ => "a envoyé un message",
    }
}

/// The body of an entry, as the chat writes it in the reader's language.
pub fn body(locale: &str, kind: &str, actor: &str, excerpt: &str) -> String {
    let language = language(locale);
    let actor = if actor.is_empty() {
        someone(language)
    } else {
        actor
    };
    match kind {
        "reaction" => format!("{actor} {}", did(kind, language)),
        "edited" if !excerpt.is_empty() => format!("{actor} {}: {excerpt}", did(kind, language)),
        _ if excerpt.is_empty() => format!("{actor} {}", did(kind, language)),
        _ => format!("{actor}: {excerpt}"),
    }
}

const NOTIFICATIONS: &str =
    "SELECT n.id, n.user_id AS account_id, n.kind, n.actor_id, n.message_id, \
    n.created_at, n.read_at, n.email_handled_at AS mail_handled_at, c.space_id AS organisation_id, \
    ch.name::text AS channel, u.display_name AS actor_name, coalesce(r.locale, '') AS locale, \
    coalesce(m.body, '') AS body, \
    (SELECT string_agg(pu.display_name, ', ' ORDER BY pu.display_name) FROM dm_participants p \
     JOIN users pu ON pu.id = p.user_id WHERE p.dm_id = c.id AND p.user_id <> n.user_id) AS others \
    FROM notifications n JOIN conversations c ON c.id = n.conversation_id \
    JOIN users r ON r.id = n.user_id LEFT JOIN channels ch ON ch.id = c.id \
    LEFT JOIN users u ON u.id = n.actor_id LEFT JOIN messages m ON m.id = n.message_id \
    WHERE n.message_id IS NOT NULL AND n.kind NOT LIKE 'calendar\\_%'";

/// Returns the VAPID key the hub should sign with, when the monolith's key opens it.
pub async fn run(
    migration: &Migration<'_>,
    source: &Source<'_>,
    model: &Model,
    report: &mut Report,
) -> Result<Option<String>, String> {
    let target = &migration.targets.hub;
    db::begin(target).await?;

    let members: Vec<Value> = model
        .members
        .iter()
        .map(|m| json!({ "organisation_id": m["organisation_id"], "account_id": m["account_id"] }))
        .collect();
    db::insert(
        target,
        "organisation_members",
        &members,
        report,
        "hub.organisation_members",
    )
    .await?;
    let presence = source
        .rows(
            "SELECT id AS account_id, manual_presence AS choice, updated_at FROM users \
             WHERE manual_presence IS NOT NULL",
        )
        .await?;
    db::insert(
        target,
        "presence_choices",
        &presence,
        report,
        "hub.presence_choices",
    )
    .await?;
    let devices = source
        .rows(
            "SELECT id, user_id AS account_id, platform, token, created_at, last_seen_at FROM push_devices",
        )
        .await?;
    db::insert(target, "push_devices", &devices, report, "hub.push_devices").await?;

    let vapid = source
        .rows(
            "SELECT vapid_private_key, vapid_private_nonce FROM instance_settings \
             WHERE vapid_private_key IS NOT NULL",
        )
        .await?;
    let vapid_pem = vapid.first().and_then(|row| {
        crypto::vapid_pem(
            &migration.keys.monolith,
            text(row, "vapid_private_key"),
            text(row, "vapid_private_nonce"),
        )
    });
    let subscriptions = source
        .rows(
            "SELECT id, user_id AS account_id, endpoint, p256dh, auth, user_agent, created_at, \
             last_success_at, failures FROM push_subscriptions",
        )
        .await?;
    if vapid_pem.is_some() {
        db::insert(
            target,
            "push_subscriptions",
            &subscriptions,
            report,
            "hub.push_subscriptions",
        )
        .await?;
    } else {
        report.drop(
            "browser push subscriptions (no VAPID key the monolith's key opens)",
            subscriptions.len() as u64,
        );
    }

    let mut settings_rows = Vec::new();
    for row in source
        .rows(
            "SELECT user_id AS account_id, notifications, updated_at FROM user_preferences \
             WHERE notifications IS NOT NULL",
        )
        .await?
    {
        let Ok(old) = serde_json::from_str::<Value>(text(&row, "notifications")) else {
            report.drop("notification settings that are not JSON", 1);
            continue;
        };
        settings_rows.push(json!({
            "account_id": row["account_id"],
            "prefs": settings(&old),
            "updated_at": row["updated_at"],
        }));
    }
    db::insert(
        target,
        "notification_settings",
        &settings_rows,
        report,
        "hub.notification_settings",
    )
    .await?;
    let levels = source
        .rows(
            "SELECT user_id AS account_id, space_id AS organisation_id, level FROM space_notification_prefs",
        )
        .await?;
    db::insert(
        target,
        "organisation_levels",
        &levels,
        report,
        "hub.organisation_levels",
    )
    .await?;

    let chat = migration
        .origins
        .get("chat")
        .filter(|_| migration.targets.chat.is_some());
    let notifications = source.rows(NOTIFICATIONS).await?;
    let mut entries = Vec::new();
    match chat {
        Some(origin) => {
            for n in &notifications {
                let organisation = text(n, "organisation_id");
                let here = match n["channel"].as_str() {
                    Some(name) => format!("#{name}"),
                    None => text(n, "others").to_owned(),
                };
                let organisation_name = model.organisation_name(organisation);
                let place = match (here.is_empty(), organisation_name.is_empty()) {
                    (true, _) => organisation_name.clone(),
                    (false, true) => here,
                    (false, false) => format!("{here} \u{b7} {organisation_name}"),
                };
                let kind = text(n, "kind");
                let quoted = excerpt(text(n, "body"));
                let actor = text(n, "actor_name");
                let mut variables = Map::new();
                let title = if place.is_empty() {
                    "Ruchoir".to_owned()
                } else {
                    place.clone()
                };
                variables.insert("title".into(), json!(title));
                variables.insert(
                    "body".into(),
                    json!(body(text(n, "locale"), kind, actor, &quoted)),
                );
                if !actor.is_empty() {
                    variables.insert("actor_name".into(), json!(actor));
                }
                if !place.is_empty() {
                    variables.insert("place".into(), json!(place));
                }
                if !quoted.is_empty() {
                    variables.insert("excerpt".into(), json!(quoted));
                }
                entries.push(json!({
                    "id": n["id"],
                    "account_id": n["account_id"],
                    "product": "chat",
                    "kind": format!("chat.{kind}"),
                    "category": category(kind),
                    "organisation_id": organisation,
                    "actor_id": n["actor_id"],
                    "object_id": n["message_id"],
                    "link": format!("{origin}/o/{organisation}/m/{}", text(n, "message_id")),
                    "variables": variables,
                    "created_at": n["created_at"],
                    "read_at": n["read_at"],
                    "mail_handled_at": n["mail_handled_at"],
                }));
            }
        }
        None => report.drop(
            "notifications about messages (the chat is not installed)",
            notifications.len() as u64,
        ),
    }
    db::insert(
        target,
        "inbox_entries",
        &entries,
        report,
        "hub.inbox_entries",
    )
    .await?;
    let calendar = db::count(
        migration.source,
        "SELECT count(*) FROM notifications WHERE kind LIKE 'calendar\\_%'",
    )
    .await?;
    report.drop(
        "calendar notifications (the events show them)",
        calendar as u64,
    );

    db::finish(target, migration.dry_run).await?;
    Ok(vapid_pem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_follow_the_old_switches() {
        let got = settings(&json!({
            "enabled": true, "mentions": false, "channelMentions": false, "reactions": false,
            "edits": false, "emailReplies": false, "quietHours": true, "quietFrom": "22:30"
        }));
        assert_eq!(got["app"]["mention"], json!(false));
        assert_eq!(got["app"]["activity"], json!(false));
        assert_eq!(got["app"]["reply"], json!(true));
        assert_eq!(got["email_categories"]["reply"], json!(false));
        assert_eq!(got["quiet_from"], json!("22:30"));
        assert_eq!(got["quiet_to"], json!("08:00"));
    }

    #[test]
    fn an_entry_reads_as_the_chat_writes_it() {
        assert_eq!(body("fr", "mention", "Ana", "Bonjour"), "Ana: Bonjour");
        assert_eq!(
            body("en", "reaction", "", ""),
            "Someone reacted to your message"
        );
        assert_eq!(excerpt("**gras** et `code`"), "gras et code");
        assert_eq!(category("dm"), "direct");
    }
}
