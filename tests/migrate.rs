//! `migrate` against real databases: a monolith built from its own migrations (the schema they leave,
//! `tests/fixtures/monolith-schema.sql`) and seeded (`monolith-seed.sql`), and the services' empty
//! databases as their migrations leave them (`tests/fixtures/<service>-schema.sql`).
//!
//! It needs a PostgreSQL it may create databases in, given as a connection string without a
//! database name in `RUCHOIR_MIGRATE_TEST_DATABASE` (for example
//! `host=127.0.0.1 port=5432 user=postgres password=postgres`); without it, the test says so and
//! passes. Each run makes its own databases and drops them at the end.

use std::path::Path;

use ruchoir_install::migrate::crypto;
use ruchoir_install::migrate::objects::{Objects, Store};
use ruchoir_install::migrate::{self, derived, Keys, Migration, Origins, Targets};
use tokio_postgres::{Client, NoTls};

const MONOLITH_KEY: [u8; 32] = [1; 32];
const PROPOLIS_KEY: [u8; 32] = [2; 32];
const CALENDAR_KEY: [u8; 32] = [3; 32];

const ANA: &str = "00000000-0000-4000-8000-00000000030a";
const BOB: &str = "00000000-0000-4000-8000-00000000030b";
const SPACE: &str = "00000000-0000-4000-8000-000000000501";
const F1: &str = "00000000-0000-4000-8000-000000000661";
const F2: &str = "00000000-0000-4000-8000-000000000662";
const F3: &str = "00000000-0000-4000-8000-000000000663";
const M1: &str = "00000000-0000-4000-8000-0000000006d1";
const M3: &str = "00000000-0000-4000-8000-0000000006d3";
const M4: &str = "00000000-0000-4000-8000-0000000006d4";
const E1: &str = "00000000-0000-4000-8000-000000000651";
const ROLE: &str = "00000000-0000-4000-8000-000000000721";
const DIRECT: &str = "00000000-0000-4000-8000-000000000641";
const GENERAL: &str = "00000000-0000-4000-8000-000000000681";

const SERVICES: [&str; 5] = ["propolis", "hub", "drive", "calendar", "chat"];

async fn connect(base: &str, database: &str) -> Client {
    let (client, connection) = tokio_postgres::connect(&format!("{base} dbname={database}"), NoTls)
        .await
        .expect("connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

async fn one(client: &Client, sql: &str) -> String {
    client
        .query_one(&format!("SELECT ({sql})::text"), &[])
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
        .get::<_, Option<String>>(0)
        .unwrap_or_default()
}

/// A database named `name` holding `schema` (a fixture), and `seed` after it.
async fn database(base: &str, admin: &Client, name: &str, schema: &str, seed: Option<&str>) {
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .await
        .expect("create database");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let client = connect(base, name).await;
    let sql = std::fs::read_to_string(fixtures.join(schema)).expect("schema");
    client.batch_execute(&sql).await.expect("schema applies");
    if let Some(seed) = seed {
        // The schema's dump empties the search path for its session: the seed gets a fresh one.
        let client = connect(base, name).await;
        let sql = std::fs::read_to_string(fixtures.join(seed)).expect("seed");
        client.batch_execute(&sql).await.expect("seed applies");
    }
}

/// What the seed cannot write in SQL: the secrets sealed under the monolith's key.
async fn seal_secrets(monolith: &Client) -> String {
    let (secret, nonce) = crypto::seal(&MONOLITH_KEY, b"JBSWY3DPEHPK3PXP").unwrap();
    monolith
        .execute(
            "INSERT INTO totp_secrets (user_id, secret_ciphertext, secret_nonce, confirmed_at) \
             VALUES ($1::text::uuid, $2, $3, now())",
            &[&ANA, &secret, &nonce],
        )
        .await
        .unwrap();
    let token = "a1b2c3";
    let (cipher, nonce) = crypto::seal(&MONOLITH_KEY, token.as_bytes()).unwrap();
    monolith
        .execute(
            "INSERT INTO calendar_event_attendees (id, event_id, email, name, status, token_digest, \
             token_cipher, token_nonce) VALUES (gen_random_uuid(), $1::text::uuid, 'eve@example.org', \
             'Eve', 'needs_action', 'dd44', $2, $3)",
            &[&E1, &cipher, &nonce],
        )
        .await
        .unwrap();
    let (vapid, nonce) = crypto::seal(&MONOLITH_KEY, &[5u8; 32]).unwrap();
    monolith
        .execute(
            "INSERT INTO instance_settings (id, vapid_private_key, vapid_private_nonce) \
             VALUES (true, $1, $2) ON CONFLICT (id) DO UPDATE \
             SET vapid_private_key = $1, vapid_private_nonce = $2",
            &[&vapid, &nonce],
        )
        .await
        .unwrap();
    token.to_owned()
}

/// The copy of the store: every object the seed names.
fn objects(root: &Path) {
    let versions = [
        (F1, "00000000-0000-4000-8000-000000000760", false),
        (F1, "00000000-0000-4000-8000-000000000761", true),
        (F2, "00000000-0000-4000-8000-000000000762", false),
        (F3, "00000000-0000-4000-8000-000000000763", false),
    ];
    for (file, version, thumb) in versions {
        let key = format!("spaces/{SPACE}/{file}/{version}");
        let path = root.join(&key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("bytes of {version}")).unwrap();
        if thumb {
            std::fs::write(root.join(format!("{key}.thumb")), "thumb").unwrap();
        }
    }
    std::fs::create_dir_all(root.join("previews")).unwrap();
    std::fs::write(root.join("previews/p1"), [0x89, b'P', b'N', b'G']).unwrap();
}

struct Instance {
    targets: Targets,
    names: Vec<String>,
}

async fn targets(base: &str, admin: &Client, suffix: &str) -> Instance {
    let mut names = Vec::new();
    for service in SERVICES {
        let name = format!("{service}_{suffix}");
        database(base, admin, &name, &format!("{service}-schema.sql"), None).await;
        names.push(name);
    }
    let open = |i: usize| connect(base, &names[i]);
    Instance {
        targets: Targets {
            propolis: open(0).await,
            hub: open(1).await,
            drive: Some(open(2).await),
            calendar: Some(open(3).await),
            chat: Some(open(4).await),
        },
        names,
    }
}

#[tokio::test]
async fn a_monolith_instance_moves_into_the_services() {
    let Ok(base) = std::env::var("RUCHOIR_MIGRATE_TEST_DATABASE") else {
        eprintln!("RUCHOIR_MIGRATE_TEST_DATABASE is unset: the migration test did not run");
        return;
    };
    let admin = connect(&base, "postgres").await;
    let suffix = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
    let source_name = format!("monolith_{suffix}");
    database(
        &base,
        &admin,
        &source_name,
        "monolith-schema.sql",
        Some("monolith-seed.sql"),
    )
    .await;
    let source = connect(&base, &source_name).await;
    let token = seal_secrets(&source).await;
    let copy = tempfile::tempdir().unwrap();
    objects(copy.path());
    let bucket = tempfile::tempdir().unwrap();
    let store = Store::Directory(bucket.path().to_owned());
    let keys = Keys {
        monolith: MONOLITH_KEY,
        propolis: PROPOLIS_KEY,
        calendar: Some(CALENDAR_KEY),
    };
    let mut origins = Origins::new();
    origins.insert("chat".into(), "https://chat.example.org".into());
    let objects = Objects::new(copy.path());

    // A dry run writes nothing.
    let dry = targets(&base, &admin, &format!("dry_{suffix}")).await;
    let outcome = migrate::run(&Migration {
        source: &source,
        targets: &dry.targets,
        keys: &keys,
        origins: &origins,
        objects: &objects,
        store: None,
        dry_run: true,
    })
    .await
    .expect("dry run");
    assert!(outcome.report.tables["propolis.users"].written == 3);
    assert_eq!(
        one(&dry.targets.propolis, "SELECT count(*) FROM users").await,
        "0"
    );
    let chat = dry.targets.chat.as_ref().unwrap();
    assert_eq!(one(chat, "SELECT count(*) FROM messages").await, "0");

    // The real run.
    let instance = targets(&base, &admin, &suffix).await;
    let t = &instance.targets;
    let migration = Migration {
        source: &source,
        targets: t,
        keys: &keys,
        origins: &origins,
        objects: &objects,
        store: Some(&store),
        dry_run: false,
    };
    let outcome = migrate::run(&migration).await.expect("migration");
    let report = outcome.report;
    println!("{report}");
    assert!(
        report.objects_missing.is_empty(),
        "{:?}",
        report.objects_missing
    );
    assert!(outcome.vapid_pem.is_some());

    // Propolis.
    let p = &t.propolis;
    assert_eq!(one(p, "SELECT count(*) FROM users").await, "3");
    assert_eq!(one(p, "SELECT count(*) FROM memberships").await, "3");
    let (secret, nonce): (Vec<u8>, Vec<u8>) = {
        let row = p
            .query_one(
                "SELECT secret_ciphertext, secret_nonce FROM totp_secrets",
                &[],
            )
            .await
            .unwrap();
        (row.get(0), row.get(1))
    };
    assert_eq!(
        crypto::open(&PROPOLIS_KEY, &secret, &nonce).unwrap(),
        b"JBSWY3DPEHPK3PXP"
    );
    let admin_role = one(
        &source,
        &format!("SELECT id FROM space_roles WHERE space_id = '{SPACE}' AND system_key = 'administrator'"),
    )
    .await;
    assert_eq!(
        one(p, &format!("SELECT id FROM roles WHERE organisation_id = '{SPACE}' AND system_key = 'administrator'")).await,
        admin_role
    );
    assert_eq!(
        one(p, &format!("SELECT count(*) FROM role_assignments WHERE user_id = '{BOB}' AND role_id = '{admin_role}'")).await,
        "1"
    );
    assert_eq!(
        one(
            p,
            &format!("SELECT permissions FROM roles WHERE id = '{ROLE}'")
        )
        .await,
        "{members.invite,journal.view}"
    );
    assert_eq!(
        one(
            p,
            "SELECT role_id FROM invitations WHERE email = 'dan@example.org'"
        )
        .await,
        admin_role
    );
    assert_eq!(
        one(
            p,
            "SELECT string_agg(action, ',' ORDER BY action) FROM organisation_journal"
        )
        .await,
        "member.joined,organisation.renamed"
    );
    assert_eq!(one(p, "SELECT count(*) FROM recovery_codes").await, "0");

    // The drive: the tree, the chat's file, and the copies a message or an event now holds.
    let d = t.drive.as_ref().unwrap();
    assert_eq!(one(d, "SELECT count(*) FROM files").await, "7");
    assert_eq!(
        one(
            d,
            &format!("SELECT holder || ' ' || holder_ref FROM files WHERE id = '{F2}'")
        )
        .await,
        format!("chat conversation:{DIRECT}")
    );
    let copy_m4 = derived(&["chat", M4, F1]).to_string();
    assert_eq!(
        one(
            d,
            &format!("SELECT holder_ref FROM files WHERE id = '{copy_m4}'")
        )
        .await,
        format!("conversation:{GENERAL}")
    );
    let key = one(
        d,
        &format!("SELECT v.storage_key FROM files f JOIN file_versions v ON v.id = f.current_version_id WHERE f.id = '{copy_m4}'"),
    )
    .await;
    assert!(key.starts_with(&format!("organisations/{SPACE}/{copy_m4}/")));
    assert!(bucket.path().join(&key).exists());
    assert!(bucket.path().join(format!("{key}.thumb")).exists());
    assert!(bucket
        .path()
        .join(format!(
            "spaces/{SPACE}/{F1}/00000000-0000-4000-8000-000000000760"
        ))
        .exists());
    assert_eq!(
        one(
            d,
            "SELECT subject_kind || ' ' || array_to_string(allow, ',') FROM file_rules"
        )
        .await,
        "person files.view"
    );
    assert_eq!(
        one(
            d,
            &format!("SELECT allow FROM role_permissions WHERE role_id = '{ROLE}'")
        )
        .await,
        "{files.manage}"
    );

    // The calendar.
    let c = t.calendar.as_ref().unwrap();
    assert_eq!(one(c, "SELECT count(*) FROM calendars").await, "2");
    assert_eq!(
        one(
            c,
            &format!("SELECT channel_ref FROM calendar_events WHERE id = '{E1}'")
        )
        .await,
        GENERAL
    );
    let row = c
        .query_one(
            "SELECT token_cipher, token_nonce FROM calendar_event_attendees WHERE email IS NOT NULL",
            &[],
        )
        .await
        .unwrap();
    let (cipher, nonce): (Vec<u8>, Vec<u8>) = (row.get(0), row.get(1));
    assert_eq!(
        crypto::open(&CALENDAR_KEY, &cipher, &nonce).unwrap(),
        token.as_bytes()
    );
    assert_eq!(
        one(c, "SELECT name || ' ' || file_id FROM calendar_event_files").await,
        format!("ordre-du-jour.txt {}", derived(&["calendar", E1, F3]))
    );
    assert_eq!(
        one(c, "SELECT token_digest FROM calendar_feed_tokens").await,
        "cc33"
    );

    // The chat.
    let ch = t.chat.as_ref().unwrap();
    assert_eq!(one(ch, "SELECT count(*) FROM messages").await, "4");
    assert_eq!(
        one(ch, "SELECT name FROM channels WHERE is_default").await,
        "général"
    );
    assert_eq!(
        one(
            ch,
            &format!("SELECT file_id FROM message_attachments WHERE message_id = '{M3}'")
        )
        .await,
        F2
    );
    assert_eq!(
        one(ch, &format!("SELECT name || ' ' || mime_type || ' ' || size_bytes FROM message_attachments WHERE message_id = '{M1}'")).await,
        "plan.png image/png 5"
    );
    assert_eq!(
        one(ch, "SELECT octet_length(image) FROM message_link_previews").await,
        "4"
    );
    assert_eq!(
        one(ch, &format!("SELECT count(*) FROM role_holdings WHERE account_id = '{BOB}' AND role_id = '{ROLE}'")).await,
        "1"
    );
    assert_eq!(
        one(
            ch,
            "SELECT count(*) FROM messages WHERE search_vector IS NOT NULL"
        )
        .await,
        "4"
    );

    // The hub.
    let h = &t.hub;
    assert_eq!(one(h, "SELECT count(*) FROM inbox_entries").await, "2");
    assert_eq!(
        one(
            h,
            &format!("SELECT link FROM inbox_entries WHERE account_id = '{BOB}'")
        )
        .await,
        format!("https://chat.example.org/o/{SPACE}/m/{M1}")
    );
    assert_eq!(
        one(
            h,
            &format!("SELECT variables->>'title' FROM inbox_entries WHERE account_id = '{BOB}'")
        )
        .await,
        "#général \u{b7} Atelier"
    );
    assert_eq!(
        one(h, &format!("SELECT prefs->'app'->>'reply' FROM notification_settings WHERE account_id = '{BOB}'")).await,
        "false"
    );
    assert_eq!(one(h, "SELECT choice FROM presence_choices").await, "dnd");
    assert_eq!(one(h, "SELECT count(*) FROM push_subscriptions").await, "1");
    assert_eq!(
        one(h, "SELECT level FROM organisation_levels").await,
        "mentions"
    );

    // Run again: nothing new, but the replicas written again whole.
    let again = migrate::run(&migration).await.expect("second run").report;
    let replicas = [
        "organisations",
        "members",
        "roles",
        "groups",
        "group_members",
        "role_holdings",
    ];
    for (name, line) in &again.tables {
        let table = name.split('.').nth(1).unwrap_or("");
        if !replicas.contains(&table) {
            assert_eq!(line.written, 0, "{name} written again");
        }
    }
    assert_eq!(again.objects.written, 0);
    assert_eq!(one(d, "SELECT count(*) FROM files").await, "7");

    // A target holding someone else is refused.
    p.batch_execute(
        "INSERT INTO users (id, email, display_name, status) \
         VALUES (gen_random_uuid(), 'zoe@example.org', 'Zoé', 'active')",
    )
    .await
    .unwrap();
    let refused = migrate::run(&migration).await.err().unwrap();
    assert!(refused.contains("fresh instance"), "{refused}");

    drop(instance.targets);
    drop(dry.targets);
    drop(source);
    let mut all = instance.names;
    all.extend(dry.names);
    all.push(source_name);
    for name in all {
        let _ = admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .await;
    }
}
