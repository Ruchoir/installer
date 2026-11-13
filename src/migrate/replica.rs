//! The copy of each organisation every product keeps (its members, roles, groups and who holds
//! which role), and what each role may do in that product.
//!
//! A product fills its copy from Propolis's change feed, which a migration does not write (its
//! events are signed, and nothing changed: the organisations are only arriving). So the migration
//! writes each product's copy itself, as the product would have from Propolis's structure of the
//! organisation, and replaces it whole on every run, as the product does. The next change in an
//! organisation makes the product read Propolis again, which then holds the same.
//!
//! What a role may do (ADR 0017 of `Ruchoir/.github`): the monolith kept one list per role for
//! everything; each product now keeps its part of it (`role_permissions`), and Propolis the
//! organisation's own part ([`super::identity`]). A permission no product knows any more is left
//! behind.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use tokio_postgres::Client;

use crate::migrate::db::{self, Source};
use crate::migrate::report::Report;
use crate::migrate::text;

/// What the drive keeps of a role's permissions.
pub const DRIVE: &[&str] = &[
    "files.view",
    "files.create",
    "files.edit",
    "files.manage",
    "files.public_link",
    "files.trash.purge",
];

/// What the calendar keeps.
pub const CALENDAR: &[&str] = &[
    "calendars.create",
    "calendar.view",
    "calendar.write",
    "calendar.manage",
];

/// What the chat keeps.
pub const CHAT: &[&str] = &[
    "channels.create",
    "channels.delete",
    "dm.start",
    "channel.view",
    "message.send",
    "message.react",
    "message.attach",
    "message.mention_all",
    "message.pin",
    "message.manage",
    "channel.members.manage",
    "channel.manage",
];

/// The monolith's organisation-wide permissions, under the names of Propolis's catalogue.
pub const PROPOLIS: &[(&str, &str)] = &[
    ("space.settings", "organisation.settings"),
    ("members.view", "members.view"),
    ("members.invite", "members.invite"),
    ("members.manage", "members.manage"),
    ("groups.manage", "groups.manage"),
    ("roles.manage", "roles.assign"),
    ("audit.view", "journal.view"),
];

/// The organisations as the monolith holds them, read once.
pub struct Model {
    pub organisations: Vec<Value>,
    pub members: Vec<Value>,
    pub roles: Vec<Value>,
    pub groups: Vec<Value>,
    pub group_members: Vec<Value>,
    pub group_roles: Vec<Value>,
    pub assignments: Vec<Value>,
}

impl Model {
    pub async fn read(source: &Source<'_>) -> Result<Self, String> {
        Ok(Self {
            organisations: source
                .rows("SELECT id, name, default_channel_id FROM spaces ORDER BY created_at, id")
                .await?,
            members: source
                .rows(
                    "SELECT m.space_id AS organisation_id, m.user_id AS account_id, m.standing, \
                     m.expires_at, u.locale, u.timezone, u.display_name AS name \
                     FROM space_members m JOIN users u ON u.id = m.user_id",
                )
                .await?,
            roles: source
                .rows(
                    "SELECT space_id AS organisation_id, id, system_key, name::text AS name, \
                     position, permissions FROM space_roles",
                )
                .await?,
            groups: source
                .rows(
                    "SELECT space_id AS organisation_id, id, name::text AS name FROM space_groups",
                )
                .await?,
            group_members: source
                .rows(
                    "SELECT space_id AS organisation_id, group_id, user_id AS account_id \
                     FROM space_group_members",
                )
                .await?,
            group_roles: source
                .rows(
                    "SELECT space_id AS organisation_id, group_id, role_id FROM space_group_roles",
                )
                .await?,
            assignments: source
                .rows(
                    "SELECT space_id AS organisation_id, user_id AS account_id, role_id \
                     FROM space_role_assignments",
                )
                .await?,
        })
    }

    /// The organisation an id belongs to, by its name (for the inbox's words).
    pub fn organisation_name(&self, organisation: &str) -> String {
        self.organisations
            .iter()
            .find(|o| text(o, "id") == organisation)
            .map(|o| text(o, "name").to_owned())
            .unwrap_or_default()
    }
}

/// The member columns a product's copy has besides the common ones.
fn member_row(product: &str, member: &Value) -> Value {
    let mut row = json!({
        "organisation_id": member["organisation_id"],
        "account_id": member["account_id"],
        "standing": member["standing"],
        "expires_at": member["expires_at"],
        "locale": member["locale"],
    });
    if matches!(product, "calendar" | "chat") {
        row["timezone"] = member["timezone"].clone();
    }
    if product == "chat" {
        row["name"] = member["name"].clone();
    }
    row
}

/// `permissions` kept by a product whose part of the catalogue is `own`.
pub fn kept(permissions: &Value, own: &[&str]) -> Vec<String> {
    permissions
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|p| own.contains(p))
        .map(str::to_owned)
        .collect()
}

/// Write `product`'s copy of every organisation into `target`, replacing what is there.
pub async fn write(
    target: &Client,
    product: &str,
    own: &[&str],
    model: &Model,
    report: &mut Report,
) -> Result<(), String> {
    let ids: Vec<String> = model
        .organisations
        .iter()
        .map(|o| format!("'{}'", text(o, "id")))
        .collect();
    if !ids.is_empty() {
        // Ids come from the source's uuid columns: they are uuids, nothing else.
        db::execute(
            target,
            &format!("DELETE FROM organisations WHERE id IN ({})", ids.join(", ")),
        )
        .await?;
    }
    let organisations: Vec<Value> = model
        .organisations
        .iter()
        .map(|o| json!({ "id": o["id"], "name": o["name"] }))
        .collect();
    let name = |table: &str| format!("{product}.{table}");
    db::insert(
        target,
        "organisations",
        &organisations,
        report,
        &name("organisations"),
    )
    .await?;
    let members: Vec<Value> = model
        .members
        .iter()
        .map(|m| member_row(product, m))
        .collect();
    db::insert(target, "members", &members, report, &name("members")).await?;
    let roles: Vec<Value> = model
        .roles
        .iter()
        .map(|r| {
            json!({
                "organisation_id": r["organisation_id"],
                "id": r["id"],
                "system_key": r["system_key"],
                "name": r["name"],
                "position": r["position"],
            })
        })
        .collect();
    db::insert(target, "roles", &roles, report, &name("roles")).await?;
    let groups: Vec<Value> = model.groups.clone();
    db::insert(target, "groups", &groups, report, &name("groups")).await?;
    db::insert(
        target,
        "group_members",
        &model.group_members,
        report,
        &name("group_members"),
    )
    .await?;

    // Who holds which role: directly, then through each group they are in.
    let mut holdings: Vec<Value> = model
        .assignments
        .iter()
        .map(|a| {
            json!({
                "organisation_id": a["organisation_id"],
                "account_id": a["account_id"],
                "role_id": a["role_id"],
                "group_id": null,
            })
        })
        .collect();
    let mut roles_of_group: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for gr in &model.group_roles {
        roles_of_group
            .entry(text(gr, "group_id"))
            .or_default()
            .push(&gr["role_id"]);
    }
    for gm in &model.group_members {
        for role in roles_of_group
            .get(text(gm, "group_id"))
            .into_iter()
            .flatten()
        {
            holdings.push(json!({
                "organisation_id": gm["organisation_id"],
                "account_id": gm["account_id"],
                "role_id": role,
                "group_id": gm["group_id"],
            }));
        }
    }
    db::insert(
        target,
        "role_holdings",
        &holdings,
        report,
        &name("role_holdings"),
    )
    .await?;

    let permissions: Vec<Value> = model
        .roles
        .iter()
        .map(|r| {
            let allow = kept(&r["permissions"], own);
            json!({
                "organisation_id": r["organisation_id"],
                "role_id": r["id"],
                "allow": allow,
            })
        })
        .collect();
    db::insert(
        target,
        "role_permissions",
        &permissions,
        report,
        &name("role_permissions"),
    )
    .await
}

/// The permissions of a role no part of the new layout keeps (none, while the catalogues cover
/// the monolith's).
pub fn orphan_permissions(model: &Model) -> u64 {
    let known = |p: &str| {
        DRIVE.contains(&p)
            || CALENDAR.contains(&p)
            || CHAT.contains(&p)
            || PROPOLIS.iter().any(|(from, _)| *from == p)
    };
    model
        .roles
        .iter()
        .flat_map(|r| r["permissions"].as_array().cloned().unwrap_or_default())
        .filter(|p| p.as_str().is_some_and(|p| !known(p)))
        .count() as u64
}
