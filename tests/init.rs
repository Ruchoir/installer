//! `init` on a throwaway directory: what it writes, and what it keeps when run again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ruchoir_install::{init, Instance};
use serde::Deserialize;

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn production() -> Instance {
    Instance::production("Example.org.", Some("admin@example.org".to_owned())).unwrap()
}

#[test]
fn a_production_instance_is_written_whole() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &production()).unwrap();

    let manifest: serde_json::Value =
        serde_json::from_str(&read(dir.path(), "manifest.json")).unwrap();
    assert_eq!(manifest["version"], 1);
    assert_eq!(manifest["domain"], "example.org");
    assert_eq!(
        manifest["services"]["propolis"]["url"],
        "https://id.example.org"
    );
    assert_eq!(
        manifest["services"]["hub"]["url"],
        "https://hub.example.org"
    );

    let compose = read(dir.path(), "compose.yaml");
    assert!(compose.contains("PROPOLIS_PUBLIC_URL: https://id.example.org"));
    assert!(compose.contains("HUB_ISSUER: https://id.example.org"));
    assert!(compose.contains("registry.ruchoir.com/propolis:latest"));
    assert!(!compose.contains("mailpit"));
    assert!(!compose.contains("SSL_CERT_FILE"));
    // PostgreSQL is ready once it answers over TCP, never on the socket its first-start setup uses.
    assert!(compose.contains(r#"test: ["CMD-SHELL", "pg_isready -h 127.0.0.1 -U ruchoir"]"#));

    let traefik = read(dir.path(), "traefik/traefik.yml");
    assert!(traefik.contains("https://acme-api.actalis.com/acme/directory"));
    assert!(traefik.contains("email: admin@example.org"));
    let routes = read(dir.path(), "traefik/dynamic.yml");
    assert!(routes.contains("Host(`id.example.org`)"));
    assert!(routes.contains("certResolver: actalis"));
    assert!(routes.contains("stsSeconds"));

    assert!(!dir.path().join("dev-ca").exists());

    // Web push: the hub signs with the VAPID key, which only it reads.
    assert!(compose.contains("HUB_VAPID_KEY: /keys/hub-vapid.pem"));
    assert!(compose.contains("./keys/hub-vapid.pem:/keys/hub-vapid.pem:ro"));
    assert!(read(dir.path(), "keys/hub-vapid.pem").starts_with("-----BEGIN PRIVATE KEY-----"));
}

#[test]
fn the_registry_holds_the_hub_with_its_public_keys_only() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &production()).unwrap();

    let registry: toml::Table = read(dir.path(), "propolis/clients.toml").parse().unwrap();
    let clients = registry["client"].as_array().unwrap();
    // The hub, then Ruchoir's two apps.
    assert_eq!(clients.len(), 3);
    let hub = &clients[0];
    assert_eq!(hub["client_id"].as_str(), Some("hub"));
    assert_eq!(hub["resource"].as_str(), Some("https://hub.example.org"));
    assert_eq!(
        hub["backchannel_logout_uri"].as_str(),
        Some("https://hub.example.org/api/v1/backchannel-logout")
    );
    assert_eq!(
        hub["grant_types"].as_array().unwrap()[0].as_str(),
        Some("client_credentials")
    );
    let keys = hub["jwks"].as_array().unwrap();
    let purposes: Vec<&str> = keys
        .iter()
        .map(|k| k["purpose"].as_str().unwrap())
        .collect();
    assert_eq!(purposes, ["client_auth", "events"]);
    for key in keys {
        let key = key.as_table().unwrap();
        assert_eq!(key["kty"].as_str(), Some("OKP"));
        assert_eq!(key["crv"].as_str(), Some("Ed25519"));
        assert_eq!(key["x"].as_str().unwrap().len(), 43);
        assert!(!key.contains_key("d"), "a private key in the registry");
    }

    // The hub's key id in compose is the one registered.
    let kid = keys[0]["kid"].as_str().unwrap();
    assert!(read(dir.path(), "compose.yaml").contains(&format!("HUB_CLIENT_KEY_ID: {kid}")));
    let scopes: Vec<&str> = hub["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(
        scopes,
        ["propolis:organizations.read", "propolis:mail.digest"]
    );
    // Propolis keeps its own keys: none is generated for it.
    assert!(!dir.path().join("keys/propolis-auth.pem").exists());
}

#[test]
fn running_init_again_keeps_the_secrets_and_the_keys() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &Instance::development()).unwrap();
    let kept = [
        ".env",
        "keys/hub-vapid.pem",
        "keys/hub-auth.pem",
        "keys/hub-events.pem",
        "dev-ca/ca.pem",
        "dev-ca/key.pem",
    ];
    let before: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
    let registry = read(dir.path(), "propolis/clients.toml");

    std::fs::remove_file(dir.path().join("compose.yaml")).unwrap();
    init(dir.path(), &Instance::development()).unwrap();

    let after: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
    assert_eq!(before, after);
    assert_eq!(registry, read(dir.path(), "propolis/clients.toml"));
    assert!(dir.path().join("compose.yaml").exists());
}

#[test]
fn the_secrets_are_long_and_distinct() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &production()).unwrap();
    let env = read(dir.path(), ".env");
    let values: Vec<&str> = env
        .lines()
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.ends_with("PASSWORD") || k.ends_with("KEY"))
        .map(|(_, v)| v)
        .collect();
    assert_eq!(values.len(), 4);
    assert!(values.iter().all(|v| v.len() >= 48));
    let key = env
        .lines()
        .find_map(|l| l.strip_prefix("PROPOLIS_SECRET_KEY="))
        .unwrap();
    assert_eq!(key.len(), 64);
    assert!(key.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut distinct = values.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), values.len());
    // The secrets stay in .env: compose refers to them by name.
    let compose = read(dir.path(), "compose.yaml");
    assert!(values.iter().all(|v| !compose.contains(v)));
}

#[cfg(unix)]
#[test]
fn private_files_are_for_their_owner_alone() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &Instance::development()).unwrap();
    for path in [
        ".env",
        "keys/hub-vapid.pem",
        "keys/hub-auth.pem",
        "keys/hub-events.pem",
        "dev-ca/key.pem",
    ] {
        let mode = std::fs::metadata(dir.path().join(path))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{path} is readable by others");
    }
}

#[test]
fn a_development_instance_runs_on_localhost_with_its_own_authority() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &Instance::development()).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    assert!(compose.contains("PROPOLIS_PUBLIC_URL: https://id.localhost"));
    assert!(compose.contains("SSL_CERT_FILE: /dev-ca/ca.pem"));
    assert!(compose.contains("PROPOLIS_SMTP_HOST: mailpit"));
    assert!(compose.contains("- id.localhost"));
    assert!(compose.contains("ruchoir/hub:dev"));
    assert!(read(dir.path(), "dev-ca/ca.pem").starts_with("-----BEGIN CERTIFICATE-----"));
    assert!(read(dir.path(), "dev-ca/cert.pem").starts_with("-----BEGIN CERTIFICATE-----"));
    let routes = read(dir.path(), "traefik/dynamic.yml");
    assert!(routes.contains("certFile: /dev-ca/cert.pem"));
    assert!(!routes.contains("stsSeconds"));
    assert!(!read(dir.path(), "traefik/traefik.yml").contains("actalis"));
}

#[test]
fn a_development_hub_wakes_no_phone_and_a_production_one_keeps_the_relay() {
    let dev = tempfile::tempdir().unwrap();
    init(dev.path(), &Instance::development()).unwrap();
    let compose = read(dev.path(), "compose.yaml");
    assert_eq!(
        compose_env(&compose, "hub", "HUB_PUSH_RELAY_URL").as_deref(),
        Some("\"\"")
    );
    let prod = tempfile::tempdir().unwrap();
    init(prod.path(), &production()).unwrap();
    let compose = read(prod.path(), "compose.yaml");
    assert_eq!(compose_env(&compose, "hub", "HUB_PUSH_RELAY_URL"), None);
}

#[test]
fn each_development_directory_is_its_own_compose_project() {
    let project = |dir: &Path| {
        let compose = read(dir, "compose.yaml");
        compose
            .lines()
            .find_map(|l| l.strip_prefix("name: "))
            .unwrap()
            .to_owned()
    };
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (
        one.path().join("ruchoir-dev"),
        two.path().join("ruchoir-dev"),
    );
    init(&a, &Instance::development()).unwrap();
    init(&b, &Instance::development()).unwrap();
    let (pa, pb) = (project(&a), project(&b));
    assert!(pa.starts_with("ruchoir-dev-") && pb.starts_with("ruchoir-dev-"));
    assert_ne!(pa, pb, "two directories share a Compose project");
    init(&a, &Instance::development()).unwrap();
    assert_eq!(project(&a), pa, "the project changed on a second run");
    let other = one.path().join("Second Try");
    init(&other, &Instance::development()).unwrap();
    assert!(project(&other).starts_with("ruchoir-second-try-"));
    let prod = tempfile::tempdir().unwrap();
    init(prod.path(), &production()).unwrap();
    assert_eq!(project(prod.path()), "ruchoir");
    assert_eq!(
        ruchoir_install::render::project_of_folder("..."),
        "ruchoir-dev"
    );
}

#[test]
fn only_domain_names_are_taken() {
    for domain in [
        "localhost",
        "",
        "exa mple.org",
        "-.org",
        "a..org",
        "https://example.org",
    ] {
        assert!(Instance::production(domain, None).is_err(), "{domain:?}");
    }
    assert_eq!(production().domain, "example.org");
}

#[test]
fn every_service_has_its_own_database() {
    let script = ruchoir_install::render::postgres_init(&production());
    for service in ["propolis", "hub"] {
        assert!(script.contains(&format!("CREATE DATABASE {service} OWNER {service};")));
    }
}

// ---- The drive ----

fn with_drive(instance: Instance) -> Instance {
    instance.with_products(&["drive"]).unwrap()
}

fn env_of(dir: &Path) -> BTreeMap<String, String> {
    read(dir, ".env")
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

const DRIVE_SECRETS: [&str; 5] = [
    "DRIVE_DB_PASSWORD",
    "GARAGE_RPC_SECRET",
    "GARAGE_ADMIN_TOKEN",
    "DRIVE_S3_ACCESS_KEY_ID",
    "DRIVE_S3_SECRET_ACCESS_KEY",
];

fn hex(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[test]
fn the_drive_gets_its_secrets_and_keys_and_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let env = env_of(dir.path());
    assert!(hex(&env["GARAGE_RPC_SECRET"], 64));
    assert!(hex(&env["GARAGE_ADMIN_TOKEN"], 64));
    assert!(hex(&env["DRIVE_S3_SECRET_ACCESS_KEY"], 64));
    assert!(hex(&env["DRIVE_DB_PASSWORD"], 48));
    let key_id = &env["DRIVE_S3_ACCESS_KEY_ID"];
    assert!(
        key_id.starts_with("GK") && hex(&key_id[2..], 24),
        "{key_id}"
    );
    assert!(read(dir.path(), "keys/drive-auth.pem").starts_with("-----BEGIN PRIVATE KEY-----"));
    assert!(read(dir.path(), "keys/drive-events.kid").starts_with("drive-events-"));

    let kept = [
        ".env",
        "keys/drive-auth.pem",
        "keys/drive-auth.kid",
        "keys/drive-events.pem",
        "keys/drive-events.kid",
    ];
    let before: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
    // Run again without naming the drive: the manifest keeps it.
    let products = ruchoir_install::products_kept(dir.path(), &[] as &[&str]).unwrap();
    assert_eq!(products, ["drive"]);
    init(dir.path(), &production().with_products(&products).unwrap()).unwrap();
    let after: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
    assert_eq!(before, after);
}

#[test]
fn the_drive_is_added_to_an_instance_without_touching_what_it_had() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &production()).unwrap();
    let env = env_of(dir.path());
    assert!(DRIVE_SECRETS.iter().all(|k| !env.contains_key(*k)));
    let hub_key = read(dir.path(), "keys/hub-auth.pem");

    let products = ruchoir_install::products_kept(dir.path(), &["drive"]).unwrap();
    init(dir.path(), &production().with_products(&products).unwrap()).unwrap();
    let added = env_of(dir.path());
    for (name, value) in &env {
        assert_eq!(&added[name], value, "{name} changed");
    }
    assert!(DRIVE_SECRETS.iter().all(|k| added.contains_key(*k)));
    assert_eq!(hub_key, read(dir.path(), "keys/hub-auth.pem"));
}

#[test]
fn the_compose_file_runs_the_drive_and_garage_with_secrets_by_name() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    let kid = read(dir.path(), "keys/drive-auth.kid");
    for line in [
        "image: ${DRIVE_IMAGE:-registry.ruchoir.com/drive:latest}",
        "DRIVE_PUBLIC_URL: https://drive.example.org",
        "DRIVE_DATABASE_URL: postgres://drive:${DRIVE_DB_PASSWORD}@postgres:5432/drive",
        "DRIVE_VALKEY_URL: redis://valkey:6379/2",
        "DRIVE_TRUSTED_PROXIES: 10.89.0.0/24",
        "DRIVE_ISSUER: https://id.example.org",
        "DRIVE_CLIENT_ID: drive",
        "DRIVE_CLIENT_KEY: /keys/drive-auth.pem",
        &format!("DRIVE_CLIENT_KEY_ID: {kid}"),
        "DRIVE_S3_ENDPOINT: http://garage:3900",
        "DRIVE_S3_REGION: garage",
        "DRIVE_S3_BUCKET: drive",
        "DRIVE_S3_ACCESS_KEY_ID: ${DRIVE_S3_ACCESS_KEY_ID}",
        "DRIVE_S3_SECRET_ACCESS_KEY: ${DRIVE_S3_SECRET_ACCESS_KEY}",
        "./keys/drive-auth.pem:/keys/drive-auth.pem:ro",
        "image: dxflrs/garage:v2.5.0",
        "GARAGE_RPC_SECRET: ${GARAGE_RPC_SECRET}",
        "GARAGE_ADMIN_TOKEN: ${GARAGE_ADMIN_TOKEN}",
        "./garage/garage.toml:/etc/garage.toml:ro",
        "garage-meta:/var/lib/garage/meta",
        "garage-data:/var/lib/garage/data",
        "DRIVE_DB_PASSWORD: ${DRIVE_DB_PASSWORD}",
        "- drive.example.org",
    ] {
        assert!(compose.contains(line), "missing: {line}");
    }
    assert!(!compose.contains("SSL_CERT_FILE"));
    // Garage is published nowhere: the proxy's ports are the only ones.
    assert_eq!(compose.matches("ports:").count(), 1);
    // No secret value is written inline.
    for (name, value) in env_of(dir.path()) {
        if name != "RUCHOIR_USER" {
            assert!(!compose.contains(&value), "{name} is inline");
        }
    }
    let garage = read(dir.path(), "garage/garage.toml");
    assert!(garage.contains("replication_factor = 1"));
    let env = env_of(dir.path());
    assert!(!garage.contains(&env["GARAGE_RPC_SECRET"]));
    assert!(!garage.contains(&env["GARAGE_ADMIN_TOKEN"]));

    let script = read(dir.path(), "postgres/init.sh");
    assert!(script.contains("CREATE ROLE drive LOGIN PASSWORD '$DRIVE_DB_PASSWORD';"));
    assert!(script.contains("CREATE DATABASE drive OWNER drive;"));

    let manifest: serde_json::Value =
        serde_json::from_str(&read(dir.path(), "manifest.json")).unwrap();
    assert_eq!(
        manifest["services"]["drive"]["url"],
        "https://drive.example.org"
    );
    let routes = read(dir.path(), "traefik/dynamic.yml");
    assert!(routes.contains("Host(`drive.example.org`)"));
    assert!(routes.contains("- url: http://drive:8080"));
}

#[test]
fn a_development_drive_is_built_here_and_trusts_the_local_authority() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(Instance::development())).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    assert!(compose.contains("image: ${DRIVE_IMAGE:-ruchoir/drive:dev}"));
    assert!(compose.contains("DRIVE_PUBLIC_URL: https://drive.localhost"));
    assert!(compose.contains("- drive.localhost"));
    // Propolis, the hub and the drive each trust the development authority.
    assert_eq!(compose.matches("SSL_CERT_FILE: /dev-ca/ca.pem").count(), 3);
    assert!(read(dir.path(), "traefik/dynamic.yml").contains("Host(`drive.localhost`)"));
    assert!(read(dir.path(), "dev-ca/names").contains("drive.localhost\n"));
}

#[test]
fn adding_the_drive_issues_the_development_certificate_again_from_the_same_authority() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &Instance::development()).unwrap();
    let ca = read(dir.path(), "dev-ca/ca.pem");
    let cert = read(dir.path(), "dev-ca/cert.pem");
    assert!(!read(dir.path(), "dev-ca/names").contains("drive.localhost"));

    init(dir.path(), &with_drive(Instance::development())).unwrap();
    assert_eq!(ca, read(dir.path(), "dev-ca/ca.pem"));
    assert_ne!(cert, read(dir.path(), "dev-ca/cert.pem"));
    assert!(read(dir.path(), "dev-ca/names").contains("drive.localhost"));

    let names = ruchoir_install::dev_ca::names("localhost", &["propolis", "hub", "drive"]);
    assert!(names.contains(&"drive.localhost".to_owned()));
    assert!(names.contains(&"id.localhost".to_owned()));
}

/// The DER bytes of a PEM certificate.
fn der(pem: &str) -> Vec<u8> {
    use base64::Engine as _;
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .unwrap()
}

fn has_extension(der: &[u8], oid: &[u8]) -> bool {
    der.windows(oid.len()).any(|w| w == oid)
}

/// The encoded object identifiers of the key usage and authority key identifier extensions.
const KEY_USAGE: [u8; 5] = [0x06, 0x03, 0x55, 0x1d, 0x0f];
const AUTHORITY_KEY_ID: [u8; 5] = [0x06, 0x03, 0x55, 0x1d, 0x23];

#[test]
fn the_development_certificates_pass_strict_verifiers() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &Instance::development()).unwrap();
    // Python 3.13 and OpenSSL's strict mode refuse an authority without key usage and a
    // certificate that does not name its authority's key.
    assert!(has_extension(
        &der(&read(dir.path(), "dev-ca/ca.pem")),
        &KEY_USAGE
    ));
    assert!(has_extension(
        &der(&read(dir.path(), "dev-ca/cert.pem")),
        &AUTHORITY_KEY_ID
    ));
    assert_eq!(
        read(dir.path(), "dev-ca/profile"),
        ruchoir_install::dev_ca::PROFILE
    );
}

#[test]
fn certificates_of_an_earlier_profile_are_issued_again_by_the_same_key() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &Instance::development()).unwrap();
    // An instance written before the profile was recorded.
    std::fs::remove_file(dir.path().join("dev-ca/profile")).unwrap();
    let key = read(dir.path(), "dev-ca/ca-key.pem");
    let ca = read(dir.path(), "dev-ca/ca.pem");
    let cert = read(dir.path(), "dev-ca/cert.pem");

    init(dir.path(), &Instance::development()).unwrap();
    assert_eq!(key, read(dir.path(), "dev-ca/ca-key.pem"));
    assert_ne!(ca, read(dir.path(), "dev-ca/ca.pem"));
    assert_ne!(cert, read(dir.path(), "dev-ca/cert.pem"));
    assert_eq!(
        read(dir.path(), "dev-ca/profile"),
        ruchoir_install::dev_ca::PROFILE
    );

    // Once current, the authority stays as it is.
    let ca = read(dir.path(), "dev-ca/ca.pem");
    init(dir.path(), &with_drive(Instance::development())).unwrap();
    assert_eq!(ca, read(dir.path(), "dev-ca/ca.pem"));
}

#[test]
fn without_the_drive_nothing_of_it_appears() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &production()).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    assert!(!compose.contains("drive") && !compose.contains("garage"));
    assert!(!read(dir.path(), "traefik/dynamic.yml").contains("drive"));
    assert!(!read(dir.path(), "postgres/init.sh").contains("drive"));
    assert!(!read(dir.path(), "propolis/clients.toml").contains("drive"));
    assert!(!read(dir.path(), "manifest.json").contains("drive"));
    assert!(!dir.path().join("garage").exists());
    assert!(!dir.path().join("keys/drive-auth.pem").exists());
    assert!(!read(dir.path(), "hub/subscriptions.toml").contains("[[subscription]]"));
    assert_eq!(
        ruchoir_install::installed_products(dir.path()).unwrap(),
        Vec::<String>::new()
    );
    // Nor the office engine, which comes with the drive alone.
    assert!(!compose.contains("office") && !compose.contains("euro-office"));
    assert!(!compose.contains("internal: true"));
    assert!(!read(dir.path(), "traefik/dynamic.yml").contains("office"));
    assert!(!dir.path().join("office").exists());

    let dev = tempfile::tempdir().unwrap();
    init(dev.path(), &Instance::development()).unwrap();
    assert!(!read(dev.path(), "dev-ca/names").contains("office"));
    assert!(!read(dev.path(), "compose.yaml").contains("office"));
    assert!(!dev.path().join("office").exists());
}

// ---- The office engine, which comes with the drive ----

#[test]
fn the_drive_comes_with_the_office_engine_on_a_network_with_no_route_out() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    for line in [
        "  office:\n    image: ghcr.io/euro-office/documentserver:v9.3.4-hotfix.1\n",
        "JWT_ENABLED: \"true\"",
        "JWT_SECRET: \"\"",
        "WOPI_ENABLED: \"true\"",
        "ALLOW_PRIVATE_IP_ADDRESS: \"true\"",
        "EXAMPLE_ENABLED: \"false\"",
        "mem_limit: ${OFFICE_MEM_LIMIT:-4g}",
        "curl -fsS http://127.0.0.1/healthcheck || exit 1",
        // A slow first start (fonts and themes) is waited for, not failed.
        "start_period: 600s",
        // The engine's network: internal, so no route out.
        "  office:\n    internal: true\n",
        // The engine is on it alone; the drive on it and on the instance's.
        "    networks: [office]\n",
        "    networks: [ruchoir, office]\n",
    ] {
        assert!(compose.contains(line), "missing: {line}");
    }
    let engine = &compose[compose.find("\n  office:\n    image:").unwrap()..];
    assert!(engine.contains("security_opt: [no-new-privileges:true]"));
    assert!(!engine.contains("ports:"), "the engine is published");
    assert_eq!(compose.matches("networks: [office]").count(), 1);
    assert_eq!(compose.matches("networks: [ruchoir, office]").count(), 1);

    // Every file the engine reads is written, unchanged, and mounted read-only where it reads it.
    for file in ruchoir_install::office::FILES {
        assert_eq!(
            std::fs::read(dir.path().join(file.path)).unwrap(),
            file.bytes,
            "{}",
            file.path
        );
        let mount = format!("- ./{}:{}:ro", file.path, file.target);
        assert!(compose.contains(&mount), "missing: {mount}");
    }
    for path in [
        "office/local-production-linux.json",
        "office/themes/theme-ruchoir-light.json",
        "office/themes/theme-ruchoir-nuage-light.json",
        "office/patches/editor-wopi.ejs",
        "office/patches/visioeditor-index.html",
    ] {
        assert!(dir.path().join(path).exists(), "{path}");
    }
    assert!(compose.contains(
        "./office/patches/editor-wopi.ejs:/var/www/euro-office/documentserver/web-apps/apps/api/wopi/editor-wopi.ejs:ro"
    ));
}

#[test]
fn the_drive_reaches_the_engine_and_browsers_reach_the_editor_through_the_drive() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    for line in [
        "DRIVE_OFFICE_URL: http://office",
        "DRIVE_OFFICE_PUBLIC_URL: https://office.example.org",
        "DRIVE_WOPI_LISTEN: 0.0.0.0:8081",
        "DRIVE_WOPI_BASE_URL: http://drive:8081",
    ] {
        assert!(compose.contains(line), "missing: {line}");
    }
    // The editor's host is its own, never the drive's: the drive refuses to start otherwise.
    assert_ne!(
        ruchoir_install::office::public_url("example.org"),
        "https://drive.example.org"
    );

    // The proxy routes the editor's host to the drive, which relays it to the engine.
    let routes = read(dir.path(), "traefik/dynamic.yml");
    assert!(routes.contains(
        "    office:\n      rule: Host(`office.example.org`)\n      entryPoints: [websecure]\n      service: drive\n      tls:\n        certResolver: actalis\n"
    ));
    // No service of its own: the engine is reached through the drive alone.
    assert!(!routes.contains("http://office"));

    // The manifest lists the products, not the editor: the drive says where it is
    // (`GET /api/v1/office`).
    let manifest: serde_json::Value =
        serde_json::from_str(&read(dir.path(), "manifest.json")).unwrap();
    assert!(manifest["services"].get("office").is_none());
    assert_eq!(
        ruchoir_install::installed_products(dir.path()).unwrap(),
        ["drive"]
    );
    // The engine has no database.
    assert!(!read(dir.path(), "postgres/init.sh").contains("office"));
}

#[test]
fn a_development_editor_is_covered_by_the_local_certificate() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(Instance::development())).unwrap();
    assert!(read(dir.path(), "dev-ca/names").contains("office.localhost\n"));
    let compose = read(dir.path(), "compose.yaml");
    assert!(compose.contains("DRIVE_OFFICE_PUBLIC_URL: https://office.localhost"));
    let routes = read(dir.path(), "traefik/dynamic.yml");
    assert!(routes.contains(
        "    office:\n      rule: Host(`office.localhost`)\n      entryPoints: [websecure]\n      service: drive\n      tls: {}\n"
    ));
    assert_eq!(
        with_drive(Instance::development()).hosted(),
        ["propolis", "hub", "drive", "office"]
    );
    assert_eq!(Instance::development().hosted(), ["propolis", "hub"]);
}

#[test]
fn the_drive_talks_to_the_hub_and_hears_propolis() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    assert!(compose.contains("DRIVE_HUB_URL: https://hub.example.org"));
    assert!(compose.contains("DRIVE_EVENTS_KEY: /keys/drive-events.pem"));
    let events_kid = read(dir.path(), "keys/drive-events.kid");
    assert!(compose.contains(&format!("DRIVE_EVENTS_KEY_ID: {}", events_kid.trim())));
    assert!(compose.contains("./keys/drive-events.pem:/keys/drive-events.pem:ro"));
    assert!(compose.contains("HUB_PAGE_ORIGINS: https://drive.example.org"));
    // The hub delivers Propolis's events to the drive, on a host it knows as the drive's pages.
    let subscriptions: toml::Table = read(dir.path(), "hub/subscriptions.toml").parse().unwrap();
    let drive = &subscriptions["subscription"].as_array().unwrap()[0];
    assert_eq!(drive["service"].as_str(), Some("drive"));
    assert_eq!(
        drive["endpoint"].as_str(),
        Some("https://drive.example.org/api/v1/events")
    );
    assert_eq!(
        drive["kinds"].as_array().unwrap()[0].as_str(),
        Some("propolis.*")
    );
    assert!(read(dir.path(), "propolis/clients.toml").contains("\"hub:publish\""));
}

#[test]
fn an_unknown_product_is_refused() {
    let error = production().with_products(&["mail"]).unwrap_err();
    assert!(error.contains("\"mail\" is not a product"), "{error}");
    let dir = tempfile::tempdir().unwrap();
    assert!(ruchoir_install::products_kept(dir.path(), &["drive", "chess"]).is_err());
    // Names are read without regard to case or spaces, and once.
    let instance = production()
        .with_products(&[" Drive", "drive", ""])
        .unwrap();
    assert_eq!(instance.products, ["drive"]);
    assert_eq!(instance.url("drive"), "https://drive.example.org");
}

// ---- Propolis's checks of its registry, rewritten here (`propolis/src/oidc/clients.rs`,
// `Registry::check`; the scopes of `discovery.rs` and `request.rs`): the installer cannot depend on
// Propolis, so a change on either side must be made on both. ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckedRegistry {
    #[serde(default, rename = "client")]
    clients: Vec<CheckedClient>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct CheckedClient {
    client_id: String,
    name: String,
    #[serde(default)]
    first_party: bool,
    #[serde(default = "private_key_jwt")]
    token_endpoint_auth_method: String,
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    redirect_uris: Vec<String>,
    #[serde(default)]
    post_logout_redirect_uris: Vec<String>,
    #[serde(default)]
    backchannel_logout_uri: Option<String>,
    #[serde(default)]
    grant_types: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    resource_scopes: Vec<String>,
    #[serde(default)]
    token_exchange: Vec<CheckedExchange>,
    #[serde(default)]
    jwks: Vec<toml::Table>,
}

/// `TokenExchange` of `clients.rs`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckedExchange {
    audience: String,
    scopes: Vec<String>,
}

fn private_key_jwt() -> String {
    "private_key_jwt".to_owned()
}

/// `GRANT_TYPES` of `clients.rs`.
const GRANT_TYPES: [&str; 4] = [
    "authorization_code",
    "refresh_token",
    "client_credentials",
    "urn:ietf:params:oauth:grant-type:token-exchange",
];
/// `SCOPES` of `discovery.rs` and `PROPOLIS_API_SCOPES` of `request.rs`.
const KNOWN_SCOPES: [&str; 9] = [
    "openid",
    "profile",
    "email",
    "offline_access",
    "organizations",
    "propolis:account",
    "propolis:organizations.read",
    "propolis:organizations.write",
    "propolis:mail.digest",
];

/// `valid_redirect` of `clients.rs`: `https`, or for an app a loopback address (any port) or its
/// private-use scheme.
fn valid_redirect(uri: &str, public: bool) -> bool {
    if uri.contains('#') || uri.contains('*') {
        return false;
    }
    if uri.starts_with("https://") {
        return uri.len() > "https://".len();
    }
    public
        && (uri.starts_with("http://127.0.0.1/")
            || uri.starts_with("http://127.0.0.1:")
            || uri.split_once(":/").is_some_and(|(scheme, _)| {
                scheme.contains('.')
                    && scheme
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'.' || b.is_ascii_digit())
            }))
}

fn check_registry(text: &str) -> Result<(), String> {
    let registry: CheckedRegistry = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut ids = BTreeSet::new();
    let mut kids = BTreeSet::new();
    // The scopes the instance's resources declare: a client may hold another resource's scope
    // (the drive holds `hub:publish`) and ask for it there.
    let declared: BTreeSet<&str> = registry
        .clients
        .iter()
        .flat_map(|c| c.resource_scopes.iter().map(String::as_str))
        .collect();
    for client in &registry.clients {
        let id = &client.client_id;
        if !ids.insert(id.clone()) {
            return Err(format!("{id} listed twice"));
        }
        // The products sign their assertions; Ruchoir's apps are public (PKCE) and hold no key.
        let public = match client.token_endpoint_auth_method.as_str() {
            "private_key_jwt" => false,
            "none" => true,
            other => return Err(format!("{id}: method {other:?}")),
        };
        if public
            && (!client.first_party
                || !client.jwks.is_empty()
                || client.resource.is_some()
                || client.grant_types.iter().any(|g| g == "client_credentials"))
        {
            return Err(format!(
                "{id}: an app is first-party, keyless and no resource"
            ));
        }
        if let Some(grant) = client
            .grant_types
            .iter()
            .find(|g| !GRANT_TYPES.contains(&g.as_str()))
        {
            return Err(format!("{id}: unknown grant {grant:?}"));
        }
        if let Some(uri) = client
            .redirect_uris
            .iter()
            .chain(&client.post_logout_redirect_uris)
            .find(|u| !valid_redirect(u, public))
        {
            return Err(format!("{id}: redirect {uri:?}"));
        }
        if client.grant_types.iter().any(|g| g == "authorization_code")
            && client.redirect_uris.is_empty()
        {
            return Err(format!("{id}: authorization_code needs a redirect URI"));
        }
        for uri in client.resource.iter().chain(&client.backchannel_logout_uri) {
            if !uri.starts_with("https://") || uri.contains('#') {
                return Err(format!("{id}: {uri:?} is not an https address"));
            }
        }
        for key in &client.jwks {
            let get = |name: &str| key.get(name).and_then(toml::Value::as_str);
            let valid = !key.contains_key("d")
                && matches!(
                    (get("kty"), get("crv")),
                    (Some("OKP"), Some("Ed25519")) | (Some("EC"), Some("P-256"))
                )
                && get("x").is_some()
                && matches!(get("purpose"), Some("client_auth" | "events"));
            if !valid {
                return Err(format!("{id}: key {:?}", get("kid")));
            }
            if !kids.insert(get("kid").unwrap_or_default().to_owned()) {
                return Err(format!("key id {:?} used twice", get("kid")));
            }
        }
        if !public
            && !client
                .jwks
                .iter()
                .any(|k| k.get("purpose").and_then(toml::Value::as_str) == Some("client_auth"))
        {
            return Err(format!("{id}: private_key_jwt needs a client_auth key"));
        }
        // Propolis grants only the scopes it knows or a resource declares: anything else would be
        // dropped silently.
        if let Some(scope) = client
            .scopes
            .iter()
            .find(|s| !KNOWN_SCOPES.contains(&s.as_str()) && !declared.contains(s.as_str()))
        {
            return Err(format!(
                "{id}: scope {scope:?} is neither Propolis's nor a resource's"
            ));
        }
        // A token is exchanged only for a resource of the instance, and for the scopes it declares.
        for exchange in &client.token_exchange {
            if !registry
                .clients
                .iter()
                .any(|c| c.resource.as_deref() == Some(exchange.audience.as_str()))
            {
                return Err(format!("{id}: {:?} is no resource", exchange.audience));
            }
            if let Some(scope) = exchange
                .scopes
                .iter()
                .find(|s| !declared.contains(s.as_str()))
            {
                return Err(format!("{id}: exchanged scope {scope:?} is no resource's"));
            }
        }
        // A resource server declares its own scopes.
        if let Some(scope) = client
            .resource_scopes
            .iter()
            .find(|s| !s.starts_with(&format!("{id}:")))
        {
            return Err(format!("{id}: resource scope {scope:?} is not its own"));
        }
    }
    Ok(())
}

#[test]
fn the_registry_with_the_drive_passes_propolis_checks() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let text = read(dir.path(), "propolis/clients.toml");
    check_registry(&text).unwrap();

    let registry: toml::Table = text.parse().unwrap();
    let clients = registry["client"].as_array().unwrap();
    assert_eq!(clients.len(), 4);
    let drive = clients[1].as_table().unwrap();
    let strings = |name: &str| -> Vec<&str> {
        drive[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect()
    };
    assert_eq!(drive["client_id"].as_str(), Some("drive"));
    assert_eq!(drive["first_party"].as_bool(), Some(true));
    assert_eq!(
        drive["resource"].as_str(),
        Some("https://drive.example.org")
    );
    assert_eq!(
        strings("grant_types"),
        ["authorization_code", "refresh_token", "client_credentials"]
    );
    assert_eq!(
        strings("redirect_uris"),
        ["https://drive.example.org/auth/callback"]
    );
    assert_eq!(
        strings("post_logout_redirect_uris"),
        ["https://drive.example.org/"]
    );
    assert_eq!(
        drive["backchannel_logout_uri"].as_str(),
        Some("https://drive.example.org/auth/backchannel-logout")
    );
    assert_eq!(
        strings("scopes"),
        [
            "openid",
            "profile",
            "email",
            "organizations",
            "propolis:organizations.read",
            "hub:publish"
        ]
    );
    assert_eq!(strings("resource_scopes"), ["drive:held.manage"]);
    let keys = drive["jwks"].as_array().unwrap();
    let kids: Vec<&str> = keys.iter().map(|k| k["kid"].as_str().unwrap()).collect();
    assert_eq!(
        kids,
        [
            read(dir.path(), "keys/drive-auth.kid").trim(),
            read(dir.path(), "keys/drive-events.kid").trim()
        ]
    );

    // The hub's entry is as it was: no empty redirect lists.
    assert!(!clients[0].as_table().unwrap().contains_key("redirect_uris"));
}

#[test]
fn the_rewritten_checks_refuse_what_propolis_refuses() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let text = read(dir.path(), "propolis/clients.toml");
    let broken = [
        text.replace(
            "https://drive.example.org/auth/callback",
            "http://drive.example.org/auth/callback",
        ),
        text.replace("\"refresh_token\"", "\"password\""),
        text.replace("first_party = true", "first_party = true\nmystery = 1"),
        text.replace("\"organizations\",", "\"organisations\","),
        text.replace("purpose = \"client_auth\"", "purpose = \"events\""),
        text.replace("\"drive:held.manage\"", "\"hub:held.manage\""),
    ];
    for text in broken {
        assert!(check_registry(&text).is_err(), "accepted:\n{text}");
    }
}

#[test]
fn garage_is_prepared_with_the_drives_key() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let env = env_of(dir.path());
    let (id, secret) = (
        &env["DRIVE_S3_ACCESS_KEY_ID"],
        &env["DRIVE_S3_SECRET_ACCESS_KEY"],
    );
    let node = "563e1ac825ee3323aa45ae8fbb0e3cfd3d84f2c63e5e5b9d4cd5c07d73f7c9ab";
    let commands: Vec<String> = ruchoir_install::garage::bootstrap(node, 0, id, secret)
        .iter()
        .map(|s| s.args.join(" "))
        .collect();
    assert_eq!(
        commands,
        [
            format!("layout assign -z dc1 -c 1T {node}"),
            "layout apply --version 1".to_owned(),
            format!("key import --yes -n drive {id} {secret}"),
            "bucket create drive".to_owned(),
            format!("bucket allow --read --write --owner drive --key {id}"),
        ]
    );
}

// ---- The calendar, alone or with the drive ----

fn with_calendar(instance: Instance) -> Instance {
    instance.with_products(&["calendar"]).unwrap()
}

fn with_drive_and_calendar(instance: Instance) -> Instance {
    instance.with_products(&["calendar", "drive"]).unwrap()
}

/// The value of `name` in the environment of compose's `service`, from its first line to the
/// blank line that ends it.
fn compose_env(compose: &str, service: &str, name: &str) -> Option<String> {
    let start = compose.find(&format!("\n  {service}:\n"))? + 1;
    let rest = &compose[start..];
    let end = rest.find("\n\n").unwrap_or(rest.len());
    rest[..end].lines().find_map(|line| {
        line.trim()
            .strip_prefix(&format!("{name}: "))
            .map(str::to_owned)
    })
}

#[test]
fn the_calendar_gets_its_secrets_and_keys_and_keeps_them() {
    for instance in [
        with_calendar(production()),
        with_drive_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let env = env_of(dir.path());
        assert!(hex(&env["CALENDAR_SECRET_KEY"], 64));
        assert!(hex(&env["CALENDAR_DB_PASSWORD"], 48));
        assert_ne!(env["CALENDAR_SECRET_KEY"], env["PROPOLIS_SECRET_KEY"]);
        assert!(
            read(dir.path(), "keys/calendar-auth.pem").starts_with("-----BEGIN PRIVATE KEY-----")
        );
        assert!(read(dir.path(), "keys/calendar-auth.kid").starts_with("calendar-auth-"));
        assert!(read(dir.path(), "keys/calendar-events.kid").starts_with("calendar-events-"));

        let kept = [
            ".env",
            "keys/calendar-auth.pem",
            "keys/calendar-auth.kid",
            "keys/calendar-events.pem",
            "keys/calendar-events.kid",
        ];
        let before: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
        // Run again without naming it: the manifest keeps it.
        let products = ruchoir_install::products_kept(dir.path(), &[] as &[&str]).unwrap();
        assert_eq!(products, instance.products);
        init(dir.path(), &production().with_products(&products).unwrap()).unwrap();
        let after: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
        assert_eq!(before, after);
    }
}

#[test]
fn the_calendar_is_added_beside_the_drive_without_touching_it() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_drive(production())).unwrap();
    let env = env_of(dir.path());
    assert!(!env.contains_key("CALENDAR_SECRET_KEY"));
    let drive_key = read(dir.path(), "keys/drive-auth.pem");

    let products = ruchoir_install::products_kept(dir.path(), &["calendar"]).unwrap();
    assert_eq!(products, ["drive", "calendar"]);
    init(dir.path(), &production().with_products(&products).unwrap()).unwrap();
    let added = env_of(dir.path());
    for (name, value) in &env {
        assert_eq!(&added[name], value, "{name} changed");
    }
    assert!(added.contains_key("CALENDAR_DB_PASSWORD"));
    assert!(added.contains_key("CALENDAR_SECRET_KEY"));
    assert_eq!(drive_key, read(dir.path(), "keys/drive-auth.pem"));
}

#[test]
fn the_compose_file_runs_the_calendar_with_secrets_by_name() {
    for instance in [
        with_calendar(production()),
        with_drive_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let compose = read(dir.path(), "compose.yaml");
        let kid = read(dir.path(), "keys/calendar-auth.kid");
        let events_kid = read(dir.path(), "keys/calendar-events.kid");
        for line in [
            "  calendar:\n    image: ${CALENDAR_IMAGE:-registry.ruchoir.com/calendar:latest}\n    user: ${RUCHOIR_USER:-10001:10001}\n",
            "CALENDAR_PUBLIC_URL: https://calendar.example.org",
            "CALENDAR_DATABASE_URL: postgres://calendar:${CALENDAR_DB_PASSWORD}@postgres:5432/calendar",
            "CALENDAR_VALKEY_URL: redis://valkey:6379/3",
            "CALENDAR_TRUSTED_PROXIES: 10.89.0.0/24",
            "CALENDAR_ISSUER: https://id.example.org",
            "CALENDAR_CLIENT_ID: calendar",
            "CALENDAR_CLIENT_KEY: /keys/calendar-auth.pem",
            &format!("CALENDAR_CLIENT_KEY_ID: {kid}"),
            "CALENDAR_HUB_URL: https://hub.example.org",
            "CALENDAR_EVENTS_KEY: /keys/calendar-events.pem",
            &format!("CALENDAR_EVENTS_KEY_ID: {events_kid}"),
            "CALENDAR_SECRET_KEY: ${CALENDAR_SECRET_KEY}",
            "./keys/calendar-auth.pem:/keys/calendar-auth.pem:ro",
            "./keys/calendar-events.pem:/keys/calendar-events.pem:ro",
            "CALENDAR_DB_PASSWORD: ${CALENDAR_DB_PASSWORD}",
            "- calendar.example.org",
        ] {
            assert!(compose.contains(line), "missing: {line}");
        }
        // Each product has a Valkey database of its own.
        assert_eq!(compose.matches("redis://valkey:6379/3").count(), 1);
        // In production the relay is the operator's (compose.override.yaml): without it, the
        // invitations are logged.
        assert!(!compose.contains("CALENDAR_SMTP_"));
        assert!(!compose.contains("SSL_CERT_FILE"));
        assert_eq!(compose.matches("ports:").count(), 1);
        for (name, value) in env_of(dir.path()) {
            if name != "RUCHOIR_USER" {
                assert!(!compose.contains(&value), "{name} is inline");
            }
        }
        // The private keys stay out of the registry and compose alike.
        let pem = read(dir.path(), "keys/calendar-auth.pem");
        let body = pem.lines().nth(1).unwrap();
        assert!(!compose.contains(body));
        assert!(!read(dir.path(), "propolis/clients.toml").contains(body));

        let drive = compose_env(&compose, "calendar", "CALENDAR_DRIVE_URL");
        if instance.has("drive") {
            assert_eq!(drive.as_deref(), Some("https://drive.example.org"));
        } else {
            assert_eq!(drive, None);
            assert!(!compose.contains("drive"));
        }

        let script = read(dir.path(), "postgres/init.sh");
        assert!(script.contains("CREATE ROLE calendar LOGIN PASSWORD '$CALENDAR_DB_PASSWORD';"));
        assert!(script.contains("CREATE DATABASE calendar OWNER calendar;"));

        let manifest: serde_json::Value =
            serde_json::from_str(&read(dir.path(), "manifest.json")).unwrap();
        assert_eq!(
            manifest["services"]["calendar"]["url"],
            "https://calendar.example.org"
        );
        assert_eq!(
            ruchoir_install::installed_products(dir.path()).unwrap(),
            instance.products
        );
        let routes = read(dir.path(), "traefik/dynamic.yml");
        assert!(routes.contains(
            "    calendar:\n      rule: Host(`calendar.example.org`)\n      entryPoints: [websecure]\n      service: calendar\n      tls:\n        certResolver: actalis\n"
        ));
        assert!(routes.contains("- url: http://calendar:8080"));
    }
}

#[test]
fn a_development_calendar_is_built_here_mails_the_catcher_and_trusts_the_local_authority() {
    for (instance, trusting) in [
        (with_calendar(Instance::development()), 3),
        (with_drive_and_calendar(Instance::development()), 4),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let compose = read(dir.path(), "compose.yaml");
        let get = |name: &str| compose_env(&compose, "calendar", name);
        assert!(compose.contains("image: ${CALENDAR_IMAGE:-ruchoir/calendar:dev}"));
        assert_eq!(
            get("CALENDAR_PUBLIC_URL").as_deref(),
            Some("https://calendar.localhost")
        );
        assert_eq!(get("CALENDAR_SMTP_HOST").as_deref(), Some("mailpit"));
        assert_eq!(get("CALENDAR_SMTP_PORT").as_deref(), Some("\"1025\""));
        assert_eq!(get("CALENDAR_SMTP_TLS").as_deref(), Some("none"));
        assert_eq!(
            get("CALENDAR_SMTP_FROM").as_deref(),
            Some("Ruchoir <no-reply@localhost>")
        );
        assert_eq!(get("SSL_CERT_FILE").as_deref(), Some("/dev-ca/ca.pem"));
        assert_eq!(
            compose.matches("SSL_CERT_FILE: /dev-ca/ca.pem").count(),
            trusting
        );
        let service = &compose[compose.find("\n  calendar:\n").unwrap()..];
        assert!(service.contains("- ./dev-ca:/dev-ca:ro"));
        assert!(compose.contains("- calendar.localhost"));
        assert!(read(dir.path(), "traefik/dynamic.yml").contains("Host(`calendar.localhost`)"));
        assert!(read(dir.path(), "dev-ca/names").contains("calendar.localhost\n"));
    }
    let names = ruchoir_install::dev_ca::names("localhost", &["propolis", "hub", "calendar"]);
    assert!(names.contains(&"calendar.localhost".to_owned()));
}

#[test]
fn the_registry_with_the_calendar_passes_propolis_checks() {
    for (instance, held) in [
        (with_calendar(production()), false),
        (with_drive_and_calendar(production()), true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let text = read(dir.path(), "propolis/clients.toml");
        check_registry(&text).unwrap();

        let registry: toml::Table = text.parse().unwrap();
        let clients = registry["client"].as_array().unwrap();
        assert_eq!(clients.len(), if held { 5 } else { 4 });
        let calendar = clients
            .iter()
            .map(|c| c.as_table().unwrap())
            .find(|c| c["client_id"].as_str() == Some("calendar"))
            .unwrap();
        let strings = |name: &str| -> Vec<&str> {
            calendar[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect()
        };
        assert_eq!(calendar["client_id"].as_str(), Some("calendar"));
        assert_eq!(calendar["first_party"].as_bool(), Some(true));
        assert_eq!(
            calendar["resource"].as_str(),
            Some("https://calendar.example.org")
        );
        assert_eq!(
            strings("grant_types"),
            ["authorization_code", "refresh_token", "client_credentials"]
        );
        assert_eq!(
            strings("redirect_uris"),
            ["https://calendar.example.org/auth/callback"]
        );
        assert_eq!(
            strings("post_logout_redirect_uris"),
            ["https://calendar.example.org/"]
        );
        assert_eq!(
            calendar["backchannel_logout_uri"].as_str(),
            Some("https://calendar.example.org/auth/backchannel-logout")
        );
        let mut scopes = vec![
            "openid",
            "profile",
            "email",
            "organizations",
            "propolis:organizations.read",
            "hub:publish",
        ];
        if held {
            scopes.push("drive:held.manage");
        }
        assert_eq!(strings("scopes"), scopes);
        // The calendar declares no scope of its own.
        assert!(strings("resource_scopes").is_empty());
        let kids: Vec<&str> = calendar["jwks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["kid"].as_str().unwrap())
            .collect();
        assert_eq!(
            kids,
            [
                read(dir.path(), "keys/calendar-auth.kid").trim(),
                read(dir.path(), "keys/calendar-events.kid").trim()
            ]
        );
    }

    // Without the drive, no resource declares `drive:held.manage`: Propolis would refuse it.
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_calendar(production())).unwrap();
    let text = read(dir.path(), "propolis/clients.toml");
    let mut registry: toml::Table = text.parse().unwrap();
    let calendar = registry["client"].as_array_mut().unwrap()[1]
        .as_table_mut()
        .unwrap();
    calendar["scopes"]
        .as_array_mut()
        .unwrap()
        .push("drive:held.manage".into());
    let asking = toml::to_string(&registry).unwrap();
    // Written again unchanged, the registry still passes: the scope alone is refused.
    check_registry(&toml::to_string(&text.parse::<toml::Table>().unwrap()).unwrap()).unwrap();
    assert!(check_registry(&asking).is_err());
}

// ---- The hub's check of its subscriptions, rewritten here (`hub/src/events.rs`,
// `Subscriptions::parse`): a change on either side must be made on both. ----

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckedSubscriptions {
    #[serde(default, rename = "subscription")]
    subscriptions: Vec<CheckedSubscription>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckedSubscription {
    service: String,
    endpoint: String,
    kinds: Vec<String>,
}

/// A kind, roughly as `ruchoir_events::Kind` takes it: dotted lowercase segments, three or more for
/// an exact kind; a pattern is a namespace followed by `.*`.
fn valid_kind(kind: &str) -> bool {
    let segment = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    match kind.strip_suffix(".*") {
        Some(namespace) => namespace.split('.').all(segment),
        None => kind.split('.').count() >= 3 && kind.split('.').all(segment),
    }
}

fn check_subscriptions(text: &str, origins: &[&str]) -> Result<CheckedSubscriptions, String> {
    let file: CheckedSubscriptions = toml::from_str(text).map_err(|e| e.to_string())?;
    for sub in &file.subscriptions {
        let on_instance = origins
            .iter()
            .any(|origin| sub.endpoint.starts_with(&format!("{origin}/")));
        if !sub.endpoint.starts_with("https://") || !on_instance {
            return Err(format!(
                "{}: {} is off the instance",
                sub.service, sub.endpoint
            ));
        }
        if let Some(kind) = sub.kinds.iter().find(|k| !valid_kind(k)) {
            return Err(format!("{}: {kind:?} is not a kind", sub.service));
        }
    }
    Ok(file)
}

#[test]
fn the_hub_delivers_to_the_calendar_and_lets_its_pages_in() {
    for instance in [
        with_calendar(production()),
        with_drive_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let compose = read(dir.path(), "compose.yaml");
        let origins = compose_env(&compose, "hub", "HUB_PAGE_ORIGINS").unwrap();
        let origins: Vec<&str> = origins.split(',').collect();
        let subscriptions =
            check_subscriptions(&read(dir.path(), "hub/subscriptions.toml"), &origins).unwrap();
        let by_service: BTreeMap<&str, (&str, Vec<&str>)> = subscriptions
            .subscriptions
            .iter()
            .map(|s| {
                (
                    s.service.as_str(),
                    (
                        s.endpoint.as_str(),
                        s.kinds.iter().map(String::as_str).collect(),
                    ),
                )
            })
            .collect();
        assert_eq!(by_service.len(), subscriptions.subscriptions.len());
        let calendar = &by_service["calendar"];
        assert_eq!(calendar.0, "https://calendar.example.org/api/v1/events");

        let drive_origins = compose_env(&compose, "drive", "DRIVE_PRODUCT_ORIGINS");
        if instance.has("drive") {
            assert_eq!(
                origins,
                ["https://drive.example.org", "https://calendar.example.org"]
            );
            assert_eq!(calendar.1, ["propolis.*", "drive.file.*"]);
            assert_eq!(
                by_service["drive"],
                (
                    "https://drive.example.org/api/v1/events",
                    vec!["propolis.*"]
                )
            );
            // The drive answers the calendar's pages with CORS on the files it holds there.
            assert_eq!(
                drive_origins.as_deref(),
                Some("https://calendar.example.org")
            );
        } else {
            assert_eq!(origins, ["https://calendar.example.org"]);
            assert_eq!(calendar.1, ["propolis.*"]);
            assert!(!by_service.contains_key("drive"));
            assert_eq!(drive_origins, None);
        }
        assert_eq!(
            ruchoir_install::render::page_origins(&instance),
            origins.join(",")
        );
    }

    // The rewritten check refuses an endpoint off the instance and a kind that is not one.
    let text = ruchoir_install::render::subscriptions(&with_calendar(production()));
    let origins = ["https://calendar.example.org"];
    assert!(check_subscriptions(&text, &origins).is_ok());
    assert!(check_subscriptions(&text, &["https://drive.example.org"]).is_err());
    assert!(check_subscriptions(&text.replace("propolis.*", "Propolis.*"), &origins).is_err());
    assert!(check_subscriptions(&text.replace("kinds", "kind"), &origins).is_err());
}

#[test]
fn without_the_calendar_nothing_of_it_appears() {
    for instance in [production(), with_drive(production())] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        for path in [
            "compose.yaml",
            "traefik/dynamic.yml",
            "postgres/init.sh",
            "propolis/clients.toml",
            "hub/subscriptions.toml",
            "manifest.json",
            ".env",
        ] {
            assert!(
                !read(dir.path(), path).to_lowercase().contains("calendar"),
                "{path} names the calendar"
            );
        }
        assert!(!dir.path().join("keys/calendar-auth.pem").exists());
        assert!(!dir.path().join("keys/calendar-events.pem").exists());
        // The drive has no other product's pages to answer.
        assert!(!read(dir.path(), "compose.yaml").contains("DRIVE_PRODUCT_ORIGINS"));
        assert!(ruchoir_install::render::drive_product_origins(&instance).is_empty());
    }
    let dev = tempfile::tempdir().unwrap();
    init(dev.path(), &with_drive(Instance::development())).unwrap();
    assert!(!read(dev.path(), "dev-ca/names").contains("calendar"));
}

// ---- The chat, which brings the drive, alone or with the calendar ----

fn with_chat(instance: Instance) -> Instance {
    instance.with_products(&["chat"]).unwrap()
}

fn with_chat_and_calendar(instance: Instance) -> Instance {
    instance.with_products(&["chat", "calendar"]).unwrap()
}

#[test]
fn choosing_the_chat_installs_the_drive() {
    assert_eq!(with_chat(production()).products, ["drive", "chat"]);
    assert_eq!(
        with_chat_and_calendar(production()).products,
        ["drive", "calendar", "chat"]
    );
    let dir = tempfile::tempdir().unwrap();
    let products = ruchoir_install::products_kept(dir.path(), &["chat"]).unwrap();
    assert_eq!(products, ["drive", "chat"]);
    init(dir.path(), &production().with_products(&products).unwrap()).unwrap();
    let env = env_of(dir.path());
    for name in DRIVE_SECRETS {
        assert!(env.contains_key(name), "{name} missing");
    }
    assert!(dir.path().join("garage/garage.toml").exists());
    assert_eq!(
        ruchoir_install::installed_products(dir.path()).unwrap(),
        ["drive", "chat"]
    );
    let compose = read(dir.path(), "compose.yaml");
    assert!(compose.contains("\n  drive:\n"));
    assert!(compose.contains("\n  garage:\n"));

    // Added to an instance with the calendar alone, the chat brings the drive with it.
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_calendar(production())).unwrap();
    let before = env_of(dir.path());
    let products = ruchoir_install::products_kept(dir.path(), &["chat"]).unwrap();
    assert_eq!(products, ["drive", "calendar", "chat"]);
    init(dir.path(), &production().with_products(&products).unwrap()).unwrap();
    let after = env_of(dir.path());
    for (name, value) in &before {
        assert_eq!(&after[name], value, "{name} changed");
    }
}

#[test]
fn the_chat_gets_its_secrets_and_keys_and_keeps_them() {
    for instance in [
        with_chat(production()),
        with_chat_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let env = env_of(dir.path());
        assert!(hex(&env["CHAT_DB_PASSWORD"], 48));
        // The chat keeps no encryption key of its own.
        assert!(!env.contains_key("CHAT_SECRET_KEY"));
        assert!(read(dir.path(), "keys/chat-auth.pem").starts_with("-----BEGIN PRIVATE KEY-----"));
        assert!(read(dir.path(), "keys/chat-auth.kid").starts_with("chat-auth-"));
        assert!(read(dir.path(), "keys/chat-events.kid").starts_with("chat-events-"));

        let kept = [
            ".env",
            "keys/chat-auth.pem",
            "keys/chat-auth.kid",
            "keys/chat-events.pem",
            "keys/chat-events.kid",
        ];
        let before: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
        let products = ruchoir_install::products_kept(dir.path(), &[] as &[&str]).unwrap();
        assert_eq!(products, instance.products);
        init(dir.path(), &production().with_products(&products).unwrap()).unwrap();
        let after: Vec<String> = kept.iter().map(|p| read(dir.path(), p)).collect();
        assert_eq!(before, after);
    }
}

#[test]
fn the_compose_file_runs_the_chat_with_secrets_by_name() {
    for instance in [
        with_chat(production()),
        with_chat_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let compose = read(dir.path(), "compose.yaml");
        let kid = read(dir.path(), "keys/chat-auth.kid");
        let events_kid = read(dir.path(), "keys/chat-events.kid");
        let get = |name: &str| compose_env(&compose, "chat", name);
        assert!(compose.contains(
            "  chat:\n    image: ${CHAT_IMAGE:-registry.ruchoir.com/chat:latest}\n    user: ${RUCHOIR_USER:-10001:10001}\n"
        ));
        for (name, value) in [
            ("CHAT_PUBLIC_URL", "https://chat.example.org"),
            (
                "CHAT_DATABASE_URL",
                "postgres://chat:${CHAT_DB_PASSWORD}@postgres:5432/chat",
            ),
            ("CHAT_VALKEY_URL", "redis://valkey:6379/4"),
            ("CHAT_TRUSTED_PROXIES", "10.89.0.0/24"),
            ("CHAT_ISSUER", "https://id.example.org"),
            ("CHAT_CLIENT_ID", "chat"),
            ("CHAT_CLIENT_KEY", "/keys/chat-auth.pem"),
            ("CHAT_CLIENT_KEY_ID", kid.as_str()),
            ("CHAT_HUB_URL", "https://hub.example.org"),
            ("CHAT_EVENTS_KEY", "/keys/chat-events.pem"),
            ("CHAT_EVENTS_KEY_ID", events_kid.as_str()),
            ("CHAT_DRIVE_URL", "https://drive.example.org"),
            ("CHAT_UNFURL_ENABLED", "\"true\""),
        ] {
            assert_eq!(get(name).as_deref(), Some(value), "{name}");
        }
        // Its own domain is never read for a preview, which covers the instance: no list needed.
        assert_eq!(get("CHAT_UNFURL_DENY_HOSTS"), None);
        // The chat sends no mail and has no key of its own in `.env`.
        assert!(!compose.contains("CHAT_SMTP_"));
        assert!(!compose.contains("CHAT_SECRET_KEY"));
        for line in [
            "./keys/chat-auth.pem:/keys/chat-auth.pem:ro",
            "./keys/chat-events.pem:/keys/chat-events.pem:ro",
            "CHAT_DB_PASSWORD: ${CHAT_DB_PASSWORD}",
            "- chat.example.org",
            "      drive: { condition: service_started }\n",
        ] {
            assert!(compose.contains(line), "missing: {line}");
        }
        assert_eq!(compose.matches("redis://valkey:6379/4").count(), 1);
        assert!(!compose.contains("SSL_CERT_FILE"));
        assert_eq!(compose.matches("ports:").count(), 1);
        for (name, value) in env_of(dir.path()) {
            if name != "RUCHOIR_USER" {
                assert!(!compose.contains(&value), "{name} is inline");
            }
        }
        let pem = read(dir.path(), "keys/chat-auth.pem");
        let body = pem.lines().nth(1).unwrap();
        assert!(!compose.contains(body));
        assert!(!read(dir.path(), "propolis/clients.toml").contains(body));

        // With the calendar, its events are posted as cards in the chat's channels.
        let chat_url = compose_env(&compose, "calendar", "CALENDAR_CHAT_URL");
        if instance.has("calendar") {
            assert_eq!(chat_url.as_deref(), Some("https://chat.example.org"));
        } else {
            assert_eq!(chat_url, None);
        }

        let script = read(dir.path(), "postgres/init.sh");
        assert!(script.contains("CREATE ROLE chat LOGIN PASSWORD '$CHAT_DB_PASSWORD';"));
        assert!(script.contains("CREATE DATABASE chat OWNER chat;"));
        let manifest: serde_json::Value =
            serde_json::from_str(&read(dir.path(), "manifest.json")).unwrap();
        assert_eq!(
            manifest["services"]["chat"]["url"],
            "https://chat.example.org"
        );
        let routes = read(dir.path(), "traefik/dynamic.yml");
        assert!(routes.contains(
            "    chat:\n      rule: Host(`chat.example.org`)\n      entryPoints: [websecure]\n      service: chat\n      tls:\n        certResolver: actalis\n"
        ));
        assert!(routes.contains("- url: http://chat:8080"));
    }
}

#[test]
fn a_development_chat_is_built_here_and_trusts_the_local_authority() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_chat(Instance::development())).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    let get = |name: &str| compose_env(&compose, "chat", name);
    assert!(compose.contains("image: ${CHAT_IMAGE:-ruchoir/chat:dev}"));
    assert_eq!(
        get("CHAT_PUBLIC_URL").as_deref(),
        Some("https://chat.localhost")
    );
    assert_eq!(get("SSL_CERT_FILE").as_deref(), Some("/dev-ca/ca.pem"));
    assert_eq!(get("CHAT_SMTP_HOST"), None);
    let service = &compose[compose.find("\n  chat:\n").unwrap()..];
    assert!(service.contains("- ./dev-ca:/dev-ca:ro"));
    assert!(compose.contains("- chat.localhost"));
    assert!(read(dir.path(), "traefik/dynamic.yml").contains("Host(`chat.localhost`)"));
    let names = read(dir.path(), "dev-ca/names");
    assert!(names.contains("chat.localhost\n"));
    assert!(names.contains("drive.localhost\n"));
}

#[test]
fn the_registry_with_the_chat_passes_propolis_checks() {
    for instance in [
        with_chat(production()),
        with_chat_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let text = read(dir.path(), "propolis/clients.toml");
        check_registry(&text).unwrap();

        let registry: toml::Table = text.parse().unwrap();
        let clients = registry["client"].as_array().unwrap();
        let by_id = |id: &str| {
            clients
                .iter()
                .map(|c| c.as_table().unwrap())
                .find(|c| c["client_id"].as_str() == Some(id))
                .unwrap_or_else(|| panic!("no client {id}"))
        };
        let strings = |client: &toml::Table, name: &str| -> Vec<String> {
            client
                .get(name)
                .and_then(toml::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect()
                })
                .unwrap_or_default()
        };
        let chat = by_id("chat");
        assert_eq!(chat["first_party"].as_bool(), Some(true));
        assert_eq!(chat["resource"].as_str(), Some("https://chat.example.org"));
        assert_eq!(
            strings(chat, "grant_types"),
            [
                "authorization_code",
                "refresh_token",
                "client_credentials",
                "urn:ietf:params:oauth:grant-type:token-exchange"
            ]
        );
        assert_eq!(
            strings(chat, "redirect_uris"),
            ["https://chat.example.org/auth/callback"]
        );
        assert_eq!(
            strings(chat, "post_logout_redirect_uris"),
            ["https://chat.example.org/"]
        );
        assert_eq!(
            chat["backchannel_logout_uri"].as_str(),
            Some("https://chat.example.org/auth/backchannel-logout")
        );
        assert_eq!(
            strings(chat, "scopes"),
            [
                "openid",
                "profile",
                "email",
                "organizations",
                "propolis:organizations.read",
                "hub:publish",
                "drive:held.manage"
            ]
        );
        assert_eq!(
            strings(chat, "resource_scopes"),
            ["chat:cards.post", "chat:channels.read"]
        );
        // The pages' socket tickets: the person's token exchanged for one for the hub.
        let exchange = chat["token_exchange"].as_array().unwrap();
        assert_eq!(exchange.len(), 1);
        assert_eq!(
            exchange[0]["audience"].as_str(),
            Some("https://hub.example.org")
        );
        assert!(exchange[0]["scopes"].as_array().unwrap().is_empty());
        let kids: Vec<&str> = chat["jwks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k["kid"].as_str().unwrap())
            .collect();
        assert_eq!(
            kids,
            [
                read(dir.path(), "keys/chat-auth.kid").trim(),
                read(dir.path(), "keys/chat-events.kid").trim()
            ]
        );
        // The other products' pages need no exchange: only the chat's ask the hub for tickets.
        for id in ["hub", "drive", "calendar"] {
            if id != "calendar" || instance.has("calendar") {
                assert!(!by_id(id).contains_key("token_exchange"), "{id}");
            }
        }
        if instance.has("calendar") {
            let scopes = strings(by_id("calendar"), "scopes");
            assert!(scopes.ends_with(&[
                "drive:held.manage".to_owned(),
                "chat:cards.post".to_owned(),
                "chat:channels.read".to_owned()
            ]));
        }
    }

    // Without the chat, no resource declares the chat's scopes, and the hub is no audience.
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &with_chat(production())).unwrap();
    let text = read(dir.path(), "propolis/clients.toml");
    for broken in [
        text.replace(
            "audience = \"https://hub.example.org\"",
            "audience = \"https://hub.example.com\"",
        ),
        text.replace("scopes = []", "scopes = [\"chat:history.read\"]"),
        text.replace("\"chat:cards.post\"", "\"calendar:cards.post\""),
    ] {
        assert!(check_registry(&broken).is_err(), "accepted:\n{broken}");
    }
}

#[test]
fn the_hub_delivers_to_the_chat_and_lets_its_pages_in() {
    for instance in [
        with_chat(production()),
        with_chat_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let compose = read(dir.path(), "compose.yaml");
        let origins = compose_env(&compose, "hub", "HUB_PAGE_ORIGINS").unwrap();
        let origins: Vec<&str> = origins.split(',').collect();
        let subscriptions =
            check_subscriptions(&read(dir.path(), "hub/subscriptions.toml"), &origins).unwrap();
        let chat = subscriptions
            .subscriptions
            .iter()
            .find(|s| s.service == "chat")
            .unwrap();
        assert_eq!(chat.endpoint, "https://chat.example.org/api/v1/events");
        assert_eq!(chat.kinds, ["propolis.*", "drive.file.*"]);
        let drive_origins = compose_env(&compose, "drive", "DRIVE_PRODUCT_ORIGINS").unwrap();
        if instance.has("calendar") {
            assert_eq!(
                origins,
                [
                    "https://drive.example.org",
                    "https://calendar.example.org",
                    "https://chat.example.org"
                ]
            );
            assert_eq!(
                drive_origins,
                "https://calendar.example.org,https://chat.example.org"
            );
        } else {
            assert_eq!(
                origins,
                ["https://drive.example.org", "https://chat.example.org"]
            );
            assert_eq!(drive_origins, "https://chat.example.org");
        }
    }
}

#[test]
fn without_the_chat_nothing_of_it_appears() {
    for instance in [
        production(),
        with_drive(production()),
        with_calendar(production()),
        with_drive_and_calendar(production()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        for path in [
            "compose.yaml",
            "traefik/dynamic.yml",
            "postgres/init.sh",
            "propolis/clients.toml",
            "hub/subscriptions.toml",
            "manifest.json",
            ".env",
        ] {
            let text = read(dir.path(), path).to_lowercase();
            assert!(!text.contains("chat"), "{path} names the chat");
            assert!(!text.contains("token_exchange"), "{path} exchanges");
        }
        assert!(!dir.path().join("keys/chat-auth.pem").exists());
        assert!(!dir.path().join("keys/chat-events.pem").exists());
    }
    let dev = tempfile::tempdir().unwrap();
    init(
        dev.path(),
        &with_drive_and_calendar(Instance::development()),
    )
    .unwrap();
    assert!(!read(dev.path(), "dev-ca/names").contains("chat"));
}

#[test]
fn the_hub_serves_the_manifest_listing_every_service_with_its_address() {
    let dir = tempfile::tempdir().unwrap();
    let instance = production().with_products(&["chat", "calendar"]).unwrap();
    init(dir.path(), &instance).unwrap();

    // The manifest lists every installed service, each with its public address.
    let manifest: serde_json::Value =
        serde_json::from_str(&read(dir.path(), "manifest.json")).unwrap();
    let services = manifest["services"].as_object().unwrap();
    let listed: BTreeSet<&str> = services.keys().map(String::as_str).collect();
    let installed: BTreeSet<&str> = instance.services().into_iter().collect();
    assert_eq!(listed, installed);
    for (name, service) in services {
        assert_eq!(service["url"], instance.url(name), "{name}");
        assert!(service["url"].as_str().unwrap().starts_with("https://"));
    }
    assert_eq!(
        services.keys().cloned().collect::<BTreeSet<_>>(),
        ["calendar", "chat", "drive", "hub", "propolis"]
            .map(str::to_owned)
            .into(),
    );

    // The hub reads it, mounted read-only, and serves it to the apps and pages.
    let compose = read(dir.path(), "compose.yaml");
    assert_eq!(
        compose_env(&compose, "hub", "HUB_MANIFEST").as_deref(),
        Some("/manifest/manifest.json")
    );
    assert!(compose.contains("      - ./manifest.json:/manifest/manifest.json:ro\n"));
}

#[test]
fn a_development_hub_serves_the_manifest_too() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), &Instance::development()).unwrap();
    let compose = read(dir.path(), "compose.yaml");
    assert_eq!(
        compose_env(&compose, "hub", "HUB_MANIFEST").as_deref(),
        Some("/manifest/manifest.json")
    );
    assert!(compose.contains("./manifest.json:/manifest/manifest.json:ro"));
}

#[test]
fn every_instance_registers_the_desktop_and_mobile_apps() {
    for instance in [production(), with_drive_and_calendar(production())] {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), &instance).unwrap();
        let text = read(dir.path(), "propolis/clients.toml");
        check_registry(&text).unwrap();
        let registry: toml::Table = text.parse().unwrap();
        let clients = registry["client"].as_array().unwrap();
        for (id, redirect) in [
            ("ruchoir-desktop", "http://127.0.0.1/callback"),
            ("ruchoir-mobile", "com.ruchoir.app:/oauth/callback"),
        ] {
            let app = clients
                .iter()
                .map(|c| c.as_table().unwrap())
                .find(|c| c["client_id"].as_str() == Some(id))
                .unwrap_or_else(|| panic!("no client {id}"));
            assert_eq!(app["token_endpoint_auth_method"].as_str(), Some("none"));
            assert_eq!(app["first_party"].as_bool(), Some(true));
            assert_eq!(
                app["redirect_uris"].as_array().unwrap(),
                &vec![toml::Value::from(redirect)]
            );
            let grants: Vec<&str> = app["grant_types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| g.as_str().unwrap())
                .collect();
            assert_eq!(grants, ["authorization_code", "refresh_token"]);
            assert!(app["scopes"]
                .as_array()
                .unwrap()
                .contains(&"offline_access".into()));
            for absent in ["resource", "jwks", "backchannel_logout_uri"] {
                assert!(!app.contains_key(absent), "{id}: {absent}");
            }
        }
    }
}

// ---- Lab, the team's tool, on a development instance ----

#[test]
fn lab_runs_on_a_development_instance_only() {
    assert!(production().with_lab(true).is_err());
    assert!(production().with_lab(false).is_ok());
    assert!(Instance::development().with_lab(true).is_ok());
}

#[test]
fn a_development_lab_signs_in_through_propolis_and_stays_out_of_the_manifest() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!ruchoir_install::lab_kept(dir.path()));
    let instance = Instance::development()
        .with_products(&["drive"])
        .unwrap()
        .with_lab(true)
        .unwrap();
    init(dir.path(), &instance).unwrap();

    // Never a product of the instance.
    let manifest: serde_json::Value =
        serde_json::from_str(&read(dir.path(), "manifest.json")).unwrap();
    assert!(manifest["services"].get("lab").is_none(), "{manifest}");
    assert_eq!(
        ruchoir_install::installed_products(dir.path()).unwrap(),
        ["drive"]
    );

    // A confidential client of Propolis, which reads the organisation it lets in.
    let registry = read(dir.path(), "propolis/clients.toml");
    check_registry(&registry).unwrap();
    let parsed: toml::Table = toml::from_str(&registry).unwrap();
    let lab = parsed["client"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["client_id"].as_str() == Some("ruchoir-lab"))
        .expect("Lab's client");
    assert_eq!(lab["resource"].as_str(), Some("https://lab.localhost"));
    assert_eq!(
        lab["redirect_uris"].as_array().unwrap()[0].as_str(),
        Some("https://lab.localhost/auth/callback")
    );
    let scopes: Vec<&str> = lab["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert!(scopes.contains(&"propolis:organizations.read"));
    assert!(!scopes.contains(&"hub:publish"));

    // Its service, database, secrets, route and certificate.
    let compose = read(dir.path(), "compose.yaml");
    let kid = read(dir.path(), "keys/lab-auth.kid");
    for line in [
        "  lab:\n    image: ${LAB_IMAGE:-ruchoir/lab:dev}\n",
        "LAB_PUBLIC_URL: https://lab.localhost",
        "LAB_DATABASE_URL: postgres://lab:${LAB_DB_PASSWORD}@postgres:5432/lab",
        "LAB_ISSUER: https://id.localhost",
        "LAB_CLIENT_ID: ruchoir-lab",
        &format!("LAB_CLIENT_KEY_ID: {}", kid.trim()),
        "LAB_ORGANISATION: ${LAB_ORGANISATION:-}",
        // The status of the servers follows the products of this instance's manifest.
        "LAB_HUB_URL: https://hub.localhost",
        "LAB_DB_PASSWORD: ${LAB_DB_PASSWORD}",
        "- ./keys/lab-auth.pem:/keys/lab-auth.pem:ro",
        "- lab.localhost",
        // Its runner, in the proxy's network, trusting the development authority.
        "  lab-runner:\n    image: ${LAB_RUNNER_IMAGE:-ruchoir/lab-runner:dev}\n",
        "network_mode: service:traefik",
        "NODE_EXTRA_CA_CERTS: /dev-ca/ca.pem",
        "- lab-captures:/srv/lab/data:ro",
        "LAB_DATA_DIR: /srv/lab/data",
        "  lab-captures:\n",
    ] {
        assert!(compose.contains(line), "missing {line:?}");
    }
    assert!(read(dir.path(), "postgres/init.sh").contains("CREATE DATABASE lab OWNER lab;"));
    assert!(env_of(dir.path())["LAB_DB_PASSWORD"].len() >= 32);
    assert!(read(dir.path(), "traefik/dynamic.yml").contains("rule: Host(`lab.localhost`)"));
    assert!(ruchoir_install::lab_kept(dir.path()));

    // Without it, nothing of Lab appears.
    let other = tempfile::tempdir().unwrap();
    init(other.path(), &Instance::development()).unwrap();
    assert!(!read(other.path(), "compose.yaml").contains("LAB_"));
    assert!(!read(other.path(), "propolis/clients.toml").contains("ruchoir-lab"));
    assert!(!ruchoir_install::lab_kept(other.path()));
}
