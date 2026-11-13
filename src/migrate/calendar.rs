//! The calendar.
//!
//! | Monolith | Calendar | How |
//! |---|---|---|
//! | `calendars` | `calendars` | `space_id` is `organisation_id`; `write_access` is gone (the monolith kept it mirrored in its rules, which come) |
//! | `permission_overrides` on a calendar | `calendar_rules` | a role, a group or a person; the calendar's permissions only |
//! | `calendar_events` | `calendar_events` | `channel_id` becomes `channel_ref`, the channel's id as text |
//! | `calendar_event_exceptions`, `calendar_attendee_overrides`, `calendar_event_reminders`, `calendar_reminder_prefs`, `calendar_reminder_deliveries`, `calendar_shares`, `calendar_visibility` | the same | as they are |
//! | `calendar_event_attendees` | `calendar_event_attendees` | the token of someone invited by address opened with the monolith's key and sealed with the calendar's, so the links already mailed keep working |
//! | `calendar_feed_tokens` | `calendar_feed_tokens` | `token_hash` is `token_digest` (the same SHA-256): the iCal addresses keep working |
//! | `calendar_event_files` | `calendar_event_files` | the file the calendar now holds for the event ([`super::drive`]), with its name and organisation |
//!
//! The cards of events posted in the chat's channels are left behind: the messages stay, as
//! ordinary messages ([`super::chat`]).

use serde_json::{json, Value};

use crate::migrate::crypto;
use crate::migrate::db::{self, Source};
use crate::migrate::drive::{self as drive_part, HeldFiles};
use crate::migrate::replica::{self, Model, CALENDAR};
use crate::migrate::report::Report;
use crate::migrate::{text, Migration};

pub async fn run(
    migration: &Migration<'_>,
    source: &Source<'_>,
    model: &Model,
    held: &HeldFiles,
    report: &mut Report,
) -> Result<(), String> {
    let Some(target) = &migration.targets.calendar else {
        let events = db::count(migration.source, "SELECT count(*) FROM calendar_events").await?;
        report.drop(
            "calendar events (the calendar is not installed)",
            events as u64,
        );
        return Ok(());
    };
    let key = migration
        .keys
        .calendar
        .ok_or("the calendar's key (CALENDAR_SECRET_KEY) is missing from .env")?;
    db::begin(target).await?;
    replica::write(target, "calendar", CALENDAR, model, report).await?;

    let plain = |table: &str, query: &str| (table.to_owned(), query.to_owned());
    let steps = [
        plain(
            "calendars",
            "SELECT id, space_id AS organisation_id, owner_user_id, name, description, color, \
             default_reminders, is_default, created_by, created_at, updated_at FROM calendars",
        ),
        plain(
            "calendar_events",
            "SELECT id, calendar_id, uid, title, description, location, all_day, start_at, end_at, \
             tzid, start_date, end_date, rrule, rdates, series_until, sequence, ical_extra, \
             created_by, updated_by, channel_id::text AS channel_ref, created_at, updated_at \
             FROM calendar_events",
        ),
        plain("calendar_event_exceptions", "SELECT * FROM calendar_event_exceptions"),
    ];
    for (table, query) in &steps {
        let rows = source.rows(query).await?;
        db::insert(target, table, &rows, report, &format!("calendar.{table}")).await?;
    }
    let rules = drive_part::rules(source, "calendar_id", CALENDAR, report).await?;
    db::insert(
        target,
        "calendar_rules",
        &rules,
        report,
        "calendar.calendar_rules",
    )
    .await?;

    // People invited by address keep their link: the token sealed again under the calendar's key.
    let mut attendees = Vec::new();
    let mut unopened = 0;
    for mut row in source
        .rows(
            "SELECT id, event_id, user_id, status, invited_by, invited_at, responded_at, email, name, \
             token_digest, token_cipher, token_nonce FROM calendar_event_attendees",
        )
        .await?
    {
        if !row["token_cipher"].is_null() {
            match crypto::reseal(
                &migration.keys.monolith,
                &key,
                text(&row, "token_cipher"),
                text(&row, "token_nonce"),
            ) {
                Some((cipher, nonce)) => {
                    row["token_cipher"] = json!(cipher);
                    row["token_nonce"] = json!(nonce);
                }
                None => {
                    unopened += 1;
                    continue;
                }
            }
        }
        attendees.push(row);
    }
    report.drop(
        "people invited by address whose token the monolith's key does not open",
        unopened,
    );
    db::insert(
        target,
        "calendar_event_attendees",
        &attendees,
        report,
        "calendar.calendar_event_attendees",
    )
    .await?;

    let rest = [
        plain("calendar_attendee_overrides", "SELECT * FROM calendar_attendee_overrides"),
        plain("calendar_event_reminders", "SELECT * FROM calendar_event_reminders"),
        plain("calendar_reminder_prefs", "SELECT * FROM calendar_reminder_prefs"),
        plain(
            "calendar_reminder_deliveries",
            "SELECT * FROM calendar_reminder_deliveries",
        ),
        plain("calendar_shares", "SELECT * FROM calendar_shares"),
        plain("calendar_visibility", "SELECT * FROM calendar_visibility"),
        plain(
            "calendar_feed_tokens",
            "SELECT id, user_id, calendar_id, token_hash AS token_digest, created_at, last_used_at, \
             revoked_at FROM calendar_feed_tokens",
        ),
    ];
    for (table, query) in &rest {
        let rows = source.rows(query).await?;
        db::insert(target, table, &rows, report, &format!("calendar.{table}")).await?;
    }

    let mut files = Vec::new();
    for ef in source
        .rows("SELECT event_id, file_id, added_by, added_at FROM calendar_event_files")
        .await?
    {
        let carrier = (
            text(&ef, "event_id").to_owned(),
            text(&ef, "file_id").to_owned(),
        );
        if let Some(file) = held.events.get(&carrier) {
            files.push(json!({
                "event_id": ef["event_id"],
                "file_id": file.file_id,
                "organisation_id": file.organisation_id,
                "name": file.name,
                "added_by": ef["added_by"],
                "added_at": ef["added_at"],
            }));
        }
    }
    db::insert(
        target,
        "calendar_event_files",
        &files,
        report,
        "calendar.calendar_event_files",
    )
    .await?;
    let cards: Value = json!(
        db::count(
            migration.source,
            "SELECT count(*) FROM messages WHERE event_id IS NOT NULL"
        )
        .await?
    );
    report.drop(
        "event cards in channels (the messages stay, without the card)",
        cards.as_u64().unwrap_or(0),
    );
    db::finish(target, migration.dry_run).await
}
