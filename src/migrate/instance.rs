//! Running a migration on an instance's directory: what `ruchoir-install migrate` does around
//! [`super::run`].
//!
//! 1. The instance must have started once (`up`): each service made its tables then. Its services
//!    are stopped; PostgreSQL and, with the drive, Garage keep running, published on the loopback
//!    alone for the time of the migration (`compose.migrate.yaml`, written here and used by this
//!    command only).
//! 2. The monolith's dump (`pg_dump --format=custom`) is restored into a database of its own,
//!    `monolith_source`, replaced on every run. It stays after the migration, for checks; the
//!    runbook says when to drop it.
//! 3. The migration runs against it ([`super::run`]); then, unless it was a dry run, the VAPID key
//!    is written to `keys/hub-vapid.pem` and the report to `migration-report.json`.
//! 4. The instance is started again as `up` starts it (the caller does it), which also takes the
//!    published ports away.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tokio_postgres::{Client, NoTls};

use crate::migrate::crypto;
use crate::migrate::objects::{Objects, Store};
use crate::migrate::{self as core, Keys, Migration, Targets};
use crate::secrets;

/// The database the dump is restored into.
pub const SOURCE_DATABASE: &str = "monolith_source";
/// The file that publishes PostgreSQL and Garage on the loopback during a migration.
pub const OVERRIDE: &str = "compose.migrate.yaml";

pub struct Options {
    pub dir: PathBuf,
    pub dump: PathBuf,
    pub objects: PathBuf,
    pub monolith_key_file: PathBuf,
    pub dry_run: bool,
    pub postgres_port: u16,
    pub garage_port: u16,
}

/// The loopback ports, as compose publishes them.
fn override_file(postgres_port: u16, garage_port: Option<u16>) -> String {
    let mut text = format!(
        "# Written by `ruchoir-install migrate` and read by it alone: PostgreSQL (and Garage) on the\n\
         # loopback while the monolith's data is carried over. `up` never reads it.\n\
         services:\n  postgres:\n    ports: [\"127.0.0.1:{postgres_port}:5432\"]\n"
    );
    if let Some(port) = garage_port {
        text.push_str(&format!(
            "  garage:\n    ports: [\"127.0.0.1:{port}:3900\"]\n"
        ));
    }
    text
}

fn compose_files(dir: &Path) -> Vec<String> {
    let mut files = vec!["-f".to_owned(), "compose.yaml".to_owned()];
    if dir.join("compose.override.yaml").exists() {
        files.extend(["-f".to_owned(), "compose.override.yaml".to_owned()]);
    }
    files.extend(["-f".to_owned(), OVERRIDE.to_owned()]);
    files
}

fn run_compose(dir: &Path, args: &[&str], stdin: Option<std::fs::File>) -> Result<String, String> {
    let mut command = Command::new("docker");
    command
        .current_dir(dir)
        .arg("compose")
        .args(compose_files(dir))
        .args(args);
    if let Some(file) = stdin {
        command.stdin(Stdio::from(file));
    }
    let output = command
        .output()
        .map_err(|e| format!("docker could not be run: {e}"))?;
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() {
        Ok(said)
    } else {
        Err(format!(
            "docker compose {}: {}",
            args.join(" "),
            said.trim()
        ))
    }
}

/// Whether `path` is a dump in pg_dump's custom format (it starts with `PGDMP`).
fn is_custom_dump(path: &Path) -> Result<bool, String> {
    let mut magic = [0u8; 5];
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(file.read_exact(&mut magic).is_ok() && &magic == b"PGDMP")
}

async fn connect(port: u16, password: &str, database: &str) -> Result<Client, String> {
    let config = format!(
        "host=127.0.0.1 port={port} user=ruchoir password={password} dbname={database} \
         application_name=ruchoir-install-migrate"
    );
    // Retried: PostgreSQL answers a moment after its container is recreated with the port.
    let mut last = String::new();
    for _ in 0..30 {
        match tokio_postgres::connect(&config, NoTls).await {
            Ok((client, connection)) => {
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                return Ok(client);
            }
            Err(e) => last = e.to_string(),
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    Err(format!("the database {database}: {last}"))
}

/// Whether a service's database holds its tables (it started once).
async fn has_tables(client: &Client) -> Result<bool, String> {
    let row = client
        .query_one(
            "SELECT count(*) FROM information_schema.tables \
             WHERE table_schema = 'public' AND table_name = 'seaql_migrations'",
            &[],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.get::<_, i64>(0) > 0)
}

/// Run a migration on the instance of `options.dir`. The caller starts the instance afterwards.
pub fn migrate(options: &Options) -> Result<String, String> {
    let dir = &options.dir;
    if !dir.join("compose.yaml").exists() {
        return Err(format!(
            "{} holds no instance: run `ruchoir-install init` and `up` first",
            dir.display()
        ));
    }
    if !is_custom_dump(&options.dump)? {
        return Err(format!(
            "{} is not a dump in pg_dump's custom format: take it with `pg_dump --format=custom`",
            options.dump.display()
        ));
    }
    if !options.objects.is_dir() {
        return Err(format!(
            "{} is not a directory: give the copy of the monolith's object store",
            options.objects.display()
        ));
    }
    let monolith = std::fs::read_to_string(&options.monolith_key_file)
        .map_err(|e| format!("{}: {e}", options.monolith_key_file.display()))
        .and_then(|hex| crypto::key_from_hex(&hex))
        .map_err(|e| format!("the monolith's key (RUCHOIR_SECRET_ENCRYPTION_KEY): {e}"))?;
    let env = secrets::read_env(&dir.join(".env"));
    let get = |name: &str| {
        env.get(name)
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or_else(|| format!("{name} is missing from .env"))
    };
    let products = crate::installed_products(dir)?;
    let has = |p: &str| products.iter().any(|q| q == p);
    let keys = Keys {
        monolith,
        propolis: crypto::key_from_hex(&get("PROPOLIS_SECRET_KEY")?)?,
        calendar: if has("calendar") {
            Some(crypto::key_from_hex(&get("CALENDAR_SECRET_KEY")?)?)
        } else {
            None
        },
    };
    let manifest: serde_json::Value = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))?;
    let origins: BTreeMap<String, String> = manifest["services"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, s)| Some((name.clone(), s["url"].as_str()?.to_owned())))
        .collect();

    // The services stop; PostgreSQL (and Garage) come up on the loopback.
    let mut services = vec!["propolis", "hub"];
    services.extend(products.iter().map(String::as_str));
    std::fs::write(
        dir.join(OVERRIDE),
        override_file(
            options.postgres_port,
            has("drive").then_some(options.garage_port),
        ),
    )
    .map_err(|e| e.to_string())?;
    let mut stop = vec!["stop"];
    stop.extend(&services);
    run_compose(dir, &stop, None)?;
    let mut up = vec!["up", "--detach", "--wait", "postgres"];
    if has("drive") {
        up.push("garage");
    }
    run_compose(dir, &up, None)?;

    // The dump, restored anew.
    run_compose(
        dir,
        &[
            "exec",
            "-T",
            "postgres",
            "dropdb",
            "-U",
            "ruchoir",
            "--if-exists",
            "--force",
            SOURCE_DATABASE,
        ],
        None,
    )?;
    run_compose(
        dir,
        &[
            "exec",
            "-T",
            "postgres",
            "createdb",
            "-U",
            "ruchoir",
            SOURCE_DATABASE,
        ],
        None,
    )?;
    let dump = std::fs::File::open(&options.dump).map_err(|e| e.to_string())?;
    run_compose(
        dir,
        &[
            "exec",
            "-T",
            "postgres",
            "pg_restore",
            "-U",
            "ruchoir",
            "--no-owner",
            "--no-privileges",
            "--exit-on-error",
            "-d",
            SOURCE_DATABASE,
        ],
        Some(dump),
    )?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let outcome = runtime.block_on(async {
        let password = get("POSTGRES_PASSWORD")?;
        let port = options.postgres_port;
        let source = connect(port, &password, SOURCE_DATABASE).await?;
        let open = |name: &'static str| {
            let password = password.clone();
            async move {
                let client = connect(port, &password, name).await?;
                if !has_tables(&client).await? {
                    return Err(format!(
                        "the {name} database has no tables yet: run `ruchoir-install up` once \
                         before migrating"
                    ));
                }
                Ok::<Client, String>(client)
            }
        };
        let targets = Targets {
            propolis: open("propolis").await?,
            hub: open("hub").await?,
            drive: if has("drive") {
                Some(open("drive").await?)
            } else {
                None
            },
            calendar: if has("calendar") {
                Some(open("calendar").await?)
            } else {
                None
            },
            chat: if has("chat") {
                Some(open("chat").await?)
            } else {
                None
            },
        };
        let store = if has("drive") && !options.dry_run {
            Some(Store::s3(
                &format!("http://127.0.0.1:{}", options.garage_port),
                crate::garage::BUCKET,
                &get("DRIVE_S3_ACCESS_KEY_ID")?,
                &get("DRIVE_S3_SECRET_ACCESS_KEY")?,
            )?)
        } else {
            None
        };
        let objects = Objects::new(&options.objects);
        core::run(&Migration {
            source: &source,
            targets: &targets,
            keys: &keys,
            origins: &origins,
            objects: &objects,
            store: store.as_ref(),
            dry_run: options.dry_run,
        })
        .await
    })?;

    let mut report = outcome.report;
    if !options.dry_run {
        if let Some(pem) = &outcome.vapid_pem {
            let path = dir.join("keys").join("hub-vapid.pem");
            secrets::write_private(&path, pem)?;
            report.note("keys/hub-vapid.pem is now the monolith's VAPID key: browsers keep their push subscriptions");
        }
        let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("migration-report.json"), json).map_err(|e| e.to_string())?;
    }
    report.note(format!(
        "the restored dump stays in the database {SOURCE_DATABASE} until you drop it"
    ));
    Ok(report.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ports_are_published_on_the_loopback_alone() {
        let text = override_file(15432, Some(13900));
        assert!(text.contains("\"127.0.0.1:15432:5432\""));
        assert!(text.contains("\"127.0.0.1:13900:3900\""));
        assert!(!override_file(15432, None).contains("garage"));
    }
}
