//! Propolis: the accounts and the organisations (the monolith's spaces).
//!
//! | Monolith | Propolis | How |
//! |---|---|---|
//! | `users` | `users` | as they are; `manual_presence` goes to the hub; `avatar_key` is left empty (Propolis keeps no pictures yet) |
//! | `totp_secrets` | `totp_secrets` | the secret opened with the monolith's key, sealed with Propolis's |
//! | `webauthn_credentials` | `webauthn_credentials` | as they are; they answer only while the relying party is the same (see the runbook) |
//! | `recovery_codes` | left behind | kept as an HMAC under the monolith's key: Propolis cannot check them; people make new ones |
//! | `spaces` | `organisations` | `icon_key` left empty, as avatars; `default_channel_id` goes to the chat |
//! | `space_slugs` | `organisation_slugs` | as they are |
//! | `space_members` | `memberships` | without the legacy `role` (the standing and the Administrator role say it) |
//! | `space_roles` | `roles` | the three system roles keep their ids (the row Propolis makes for a new organisation takes the monolith's id); a role's permissions are those of Propolis's catalogue, renamed (`space.settings` to `organisation.settings`, `roles.manage` to `roles.assign`, `audit.view` to `journal.view`); the rest goes to the products ([`super::replica`]) |
//! | `space_role_assignments` | `role_assignments` | as they are |
//! | `space_groups`, `space_group_members`, `space_group_roles` | `groups`, `group_members`, `group_roles` | as they are |
//! | `space_invitations` | `invitations` | `role` becomes a standing (`guest`, or `member`) and, for `admin`, the Administrator role; an invitation to a channel loses its channel (no product reads an invitation's context yet) |
//! | `space_audit_log` | `organisation_journal` | the entries about the organisation, its members, roles and groups (`space.*` actions become `organisation.*`); those about a channel, a calendar, a file or a licence are left behind |
//!
//! Left behind for good: the monolith's sessions and tokens (everyone signs in again), its instance
//! settings and events, and its licence tables.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::migrate::crypto;
use crate::migrate::db::{self, Source};
use crate::migrate::replica::{Model, PROPOLIS};
use crate::migrate::report::Report;
use crate::migrate::{text, Migration};

/// Refuse a target that holds accounts or organisations the source does not: a migration fills a
/// fresh instance.
pub async fn guard(migration: &Migration<'_>) -> Result<(), String> {
    for (table, source_table) in [("users", "users"), ("organisations", "spaces")] {
        let source: BTreeSet<String> = db::rows(
            migration.source,
            &format!("SELECT id::text AS id FROM {source_table}"),
        )
        .await?
        .iter()
        .map(|r| text(r, "id").to_owned())
        .collect();
        let target = db::rows(
            &migration.targets.propolis,
            &format!("SELECT id::text AS id FROM {table}"),
        )
        .await?;
        let foreign = target
            .iter()
            .filter(|r| !source.contains(text(r, "id")))
            .count();
        if foreign > 0 {
            return Err(format!(
                "Propolis already holds {foreign} {table} the monolith does not: migrate into a \
                 fresh instance (a new directory, `init` then `up`), never into one in use"
            ));
        }
    }
    Ok(())
}

/// The permissions of Propolis's catalogue a monolith role held.
fn organisation_permissions(permissions: &Value) -> Vec<String> {
    permissions
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|p| {
            PROPOLIS
                .iter()
                .find(|(from, _)| *from == p)
                .map(|(_, to)| (*to).to_owned())
        })
        .collect()
}

pub async fn run(
    migration: &Migration<'_>,
    source: &Source<'_>,
    model: &Model,
    report: &mut Report,
) -> Result<(), String> {
    let target = &migration.targets.propolis;
    db::begin(target).await?;

    let users = source
        .rows(
            "SELECT id, email, display_name, password_hash, status, mfa_enforced, created_at, \
             updated_at, title, pronouns, timezone, bio, is_bot, is_instance_admin, locale FROM users",
        )
        .await?;
    db::insert(target, "users", &users, report, "propolis.users").await?;
    let avatars = db::count(
        migration.source,
        "SELECT count(*) FROM users WHERE avatar_key IS NOT NULL",
    )
    .await?;
    report.drop("profile pictures (Propolis keeps none yet)", avatars as u64);

    // Two-factor secrets, sealed again under Propolis's key.
    let mut secrets = Vec::new();
    let mut unopened = 0;
    for row in source.rows("SELECT * FROM totp_secrets").await? {
        match crypto::reseal(
            &migration.keys.monolith,
            &migration.keys.propolis,
            text(&row, "secret_ciphertext"),
            text(&row, "secret_nonce"),
        ) {
            Some((ciphertext, nonce)) => {
                let mut row = row;
                row["secret_ciphertext"] = json!(ciphertext);
                row["secret_nonce"] = json!(nonce);
                secrets.push(row);
            }
            None => unopened += 1,
        }
    }
    report.drop("TOTP secrets the monolith's key does not open", unopened);
    db::insert(
        target,
        "totp_secrets",
        &secrets,
        report,
        "propolis.totp_secrets",
    )
    .await?;
    let codes = db::count(
        migration.source,
        "SELECT count(DISTINCT user_id) FROM recovery_codes WHERE used_at IS NULL",
    )
    .await?;
    if codes > 0 {
        report.drop(
            "recovery codes (kept under the monolith's key)",
            codes as u64,
        );
        report.note(format!(
            "{codes} account(s) had unused recovery codes: they make new ones in their security \
             settings"
        ));
    }
    let passkeys = source.rows("SELECT * FROM webauthn_credentials").await?;
    db::insert(
        target,
        "webauthn_credentials",
        &passkeys,
        report,
        "propolis.webauthn_credentials",
    )
    .await?;
    if !passkeys.is_empty() {
        report.note(format!(
            "{} passkey(s) carried over: they work only if the monolith's RUCHOIR_WEBAUTHN_RP_ID \
             is the host Propolis answers on (id.<domain>) or a domain it belongs to",
            passkeys.len()
        ));
    }

    // Organisations: inserting one makes its three system roles, which then take the monolith's ids.
    let organisations = source
        .rows("SELECT id, name, slug, created_by, created_at, updated_at FROM spaces")
        .await?;
    db::insert(
        target,
        "organisations",
        &organisations,
        report,
        "propolis.organisations",
    )
    .await?;
    let icons = db::count(
        migration.source,
        "SELECT count(*) FROM spaces WHERE icon_key IS NOT NULL",
    )
    .await?;
    report.drop("organisation icons (Propolis keeps none yet)", icons as u64);
    let slugs = source
        .rows("SELECT slug, space_id AS organisation_id, created_at FROM space_slugs")
        .await?;
    db::insert(
        target,
        "organisation_slugs",
        &slugs,
        report,
        "propolis.organisation_slugs",
    )
    .await?;

    let mut custom = Vec::new();
    for role in source
        .rows(
            "SELECT id, space_id AS organisation_id, system_key, name::text AS name, colour, \
             position, permissions, created_by, created_at, updated_at FROM space_roles",
        )
        .await?
    {
        let permissions = organisation_permissions(&role["permissions"]);
        match role["system_key"].as_str() {
            Some(key) => {
                // The Administrator allows Propolis's whole catalogue, as Propolis made it.
                let sql = if key == "administrator" {
                    "UPDATE roles SET id = $1::text::uuid \
                     WHERE organisation_id = $2::text::uuid AND system_key = $3 AND id <> $1::text::uuid"
                } else {
                    "UPDATE roles SET id = $1::text::uuid, permissions = $4 \
                     WHERE organisation_id = $2::text::uuid AND system_key = $3 AND id <> $1::text::uuid"
                };
                let (id, organisation) = (text(&role, "id"), text(&role, "organisation_id"));
                let changed = if key == "administrator" {
                    target.execute(sql, &[&id, &organisation, &key]).await
                } else {
                    target
                        .execute(sql, &[&id, &organisation, &key, &permissions])
                        .await
                }
                .map_err(|e| format!("system role of {organisation}: {e}"))?;
                let line = report.line("propolis.roles (system)");
                line.read += 1;
                if changed > 0 {
                    line.written += 1;
                } else {
                    line.kept += 1;
                }
            }
            None => {
                let mut role = role;
                role["permissions"] = json!(permissions);
                custom.push(role);
            }
        }
    }
    db::insert(target, "roles", &custom, report, "propolis.roles").await?;

    let memberships = source
        .rows(
            "SELECT space_id AS organisation_id, user_id, standing, invited_by, joined_at, \
             expires_at, expiry_notice_sent_at, expiry_stage, imported_from FROM space_members",
        )
        .await?;
    db::insert(
        target,
        "memberships",
        &memberships,
        report,
        "propolis.memberships",
    )
    .await?;
    let assignments = source
        .rows("SELECT space_id AS organisation_id, user_id, role_id FROM space_role_assignments")
        .await?;
    db::insert(
        target,
        "role_assignments",
        &assignments,
        report,
        "propolis.role_assignments",
    )
    .await?;
    let groups = source
        .rows(
            "SELECT id, space_id AS organisation_id, name, description, source, external_id, \
             created_by, imported_from, created_at, updated_at FROM space_groups",
        )
        .await?;
    db::insert(target, "groups", &groups, report, "propolis.groups").await?;
    let members = source
        .rows(
            "SELECT group_id, space_id AS organisation_id, user_id, is_manager, added_by, added_at \
             FROM space_group_members",
        )
        .await?;
    db::insert(
        target,
        "group_members",
        &members,
        report,
        "propolis.group_members",
    )
    .await?;
    let roles = source
        .rows("SELECT group_id, space_id AS organisation_id, role_id FROM space_group_roles")
        .await?;
    db::insert(
        target,
        "group_roles",
        &roles,
        report,
        "propolis.group_roles",
    )
    .await?;

    let invitations = source
        .rows(
            "SELECT i.id, i.space_id AS organisation_id, i.token_hash, i.email, \
             CASE i.role WHEN 'guest' THEN 'guest' ELSE 'member' END AS standing, \
             CASE i.role WHEN 'admin' THEN r.id END AS role_id, \
             i.created_by, i.max_uses, i.uses, i.expires_at, i.revoked_at, i.created_at, \
             CASE i.role WHEN 'guest' THEN i.guest_expires_at END AS guest_expires_at \
             FROM space_invitations i LEFT JOIN space_roles r \
             ON r.space_id = i.space_id AND r.system_key = 'administrator'",
        )
        .await?;
    db::insert(
        target,
        "invitations",
        &invitations,
        report,
        "propolis.invitations",
    )
    .await?;
    let to_channels = db::count(
        migration.source,
        "SELECT count(*) FROM space_invitations WHERE channel_id IS NOT NULL \
         AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())",
    )
    .await?;
    if to_channels > 0 {
        report.note(format!(
            "{to_channels} open invitation(s) named a channel: they still let people into the \
             organisation, and the channel must be joined by hand"
        ));
    }

    let journal = source
        .rows(
            "SELECT id, space_id AS organisation_id, at, actor_id, \
             regexp_replace(action, '^space\\.', 'organisation.') AS action, \
             CASE target_kind WHEN 'space' THEN 'organisation' ELSE target_kind END AS target_kind, \
             target_id, details FROM space_audit_log \
             WHERE target_kind IN ('space', 'member', 'role', 'group')",
        )
        .await?;
    db::insert(
        target,
        "organisation_journal",
        &journal,
        report,
        "propolis.organisation_journal",
    )
    .await?;
    let other = db::count(
        migration.source,
        "SELECT count(*) FROM space_audit_log \
         WHERE target_kind NOT IN ('space', 'member', 'role', 'group')",
    )
    .await?;
    report.drop(
        "journal entries about a channel, a calendar, a file or a licence",
        other as u64,
    );
    let orphans = crate::migrate::replica::orphan_permissions(model);
    report.drop("role permissions no service keeps", orphans);

    db::finish(target, migration.dry_run).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_keeps_the_organisation_part_under_propolis_names() {
        let got = organisation_permissions(&json!([
            "space.settings",
            "roles.manage",
            "audit.view",
            "files.view",
            "members.view"
        ]));
        assert_eq!(
            got,
            vec![
                "organisation.settings",
                "roles.assign",
                "journal.view",
                "members.view"
            ]
        );
    }
}
