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
    assert_eq!(
        ruchoir_install::installed_products(dir.path()).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn an_unknown_product_is_refused() {
    let error = production().with_products(&["calendar"]).unwrap_err();
    assert!(error.contains("\"calendar\" is not a product"), "{error}");
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
    token_exchange: Vec<toml::Value>,
    #[serde(default)]
    jwks: Vec<toml::Table>,
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

fn valid_redirect(uri: &str) -> bool {
    !uri.contains('#')
        && !uri.contains('*')
        && uri.starts_with("https://")
        && uri.len() > "https://".len()
}

fn check_registry(text: &str) -> Result<(), String> {
    let registry: CheckedRegistry = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut ids = BTreeSet::new();
    let mut kids = BTreeSet::new();
    for client in &registry.clients {
        let id = &client.client_id;
        if !ids.insert(id.clone()) {
            return Err(format!("{id} listed twice"));
        }
        if client.token_endpoint_auth_method != "private_key_jwt" {
            return Err(format!("{id}: the installer registers products only"));
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
            .find(|u| !valid_redirect(u))
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
        if !client
            .jwks
            .iter()
            .any(|k| k.get("purpose").and_then(toml::Value::as_str) == Some("client_auth"))
        {
            return Err(format!("{id}: private_key_jwt needs a client_auth key"));
        }
        // Propolis grants only the scopes it knows: anything else would be dropped silently.
        if let Some(scope) = client
            .scopes
            .iter()
            .find(|s| !KNOWN_SCOPES.contains(&s.as_str()))
        {
            return Err(format!("{id}: scope {scope:?} is not Propolis's"));
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
    assert_eq!(clients.len(), 2);
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
            "propolis:organizations.read"
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
