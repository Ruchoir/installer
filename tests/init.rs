//! `init` on a throwaway directory: what it writes, and what it keeps when run again.

use std::path::Path;

use ruchoir_install::{init, Instance};

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
    assert_eq!(clients.len(), 1);
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
    let script = ruchoir_install::render::postgres_init();
    for service in ["propolis", "hub"] {
        assert!(script.contains(&format!("CREATE DATABASE {service} OWNER {service};")));
    }
}
