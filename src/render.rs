//! The files of an instance, written from it and its secrets.
//!
//! - `manifest.json`: the services installed and their addresses (ADR 0004), versioned. Lab is never
//!   listed: it is the team's tool, not a product of the instance.
//! - `propolis/clients.toml`: the clients of the instance, with their public keys (Propolis's design,
//!   section 9.2): the hub, each product installed, and Lab when it runs. The chat may exchange a person's token for
//!   one for the hub, which its pages' socket tickets are asked with.
//! - `hub/subscriptions.toml`: which service receives which events, for the products installed.
//! - `postgres/init.sh`: one database and one role per service.
//! - `traefik/traefik.yml` and `traefik/dynamic.yml`: the proxy, routing each host to its service by
//!   file (no Docker socket in the proxy), certificates by ACME from Actalis, or the development
//!   certificate; access logs without query strings, where codes travel.
//! - `garage/garage.toml`, with the drive: its object store (see [`crate::garage`]).
//! - `office/`, with the drive: the files Euro-Office mounts (see [`crate::office`]), written as
//!   copied from the monolith, since none names anything of the instance.
//! - `compose.yaml`: everything above, run together.

use std::path::Path;

use serde::Serialize;
use serde_json::json;

use crate::secrets::Secrets;
use crate::Instance;

/// The manifest's format version.
pub const MANIFEST_VERSION: u32 = 1;
/// The network the containers share, so Propolis knows its proxy (`PROPOLIS_TRUSTED_PROXIES`).
pub const SUBNET: &str = "10.89.0.0/24";
/// Actalis's ACME directory (Italy): the certificate authority the installer asks (decision of
/// 2026-11-08), rather than one outside the dependency rules.
pub const ACTALIS_ACME: &str = "https://acme-api.actalis.com/acme/directory";

fn write(dir: &Path, path: &str, text: &str) -> Result<(), String> {
    write_bytes(dir, path, text.as_bytes())
}

fn write_bytes(dir: &Path, path: &str, bytes: &[u8]) -> Result<(), String> {
    let full = dir.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&full, bytes).map_err(|e| format!("{}: {e}", full.display()))
}

/// The manifest.
pub fn manifest(instance: &Instance) -> String {
    let services: serde_json::Map<String, serde_json::Value> = instance
        .listed()
        .into_iter()
        .map(|s| (s.to_owned(), json!({ "url": instance.url(s) })))
        .collect();
    let manifest = json!({
        "version": MANIFEST_VERSION,
        "domain": instance.domain,
        "services": services,
    });
    serde_json::to_string_pretty(&manifest).expect("a manifest serialises") + "\n"
}

#[derive(Serialize)]
struct Registry {
    client: Vec<RegistryClient>,
}

#[derive(Serialize)]
struct RegistryClient {
    client_id: String,
    name: String,
    first_party: bool,
    /// `private_key_jwt` for the products (Propolis's default), `none` for the apps (PKCE).
    #[serde(skip_serializing_if = "Option::is_none")]
    token_endpoint_auth_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    redirect_uris: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    post_logout_redirect_uris: Vec<String>,
    grant_types: Vec<String>,
    scopes: Vec<String>,
    resource_scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backchannel_logout_uri: Option<String>,
    /// The audiences it may exchange a person's token for (RFC 8693), and the scopes it may ask
    /// there. Before the keys: TOML writes its arrays of tables last.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    token_exchange: Vec<RegistryExchange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    jwks: Vec<RegistryKey>,
}

/// Ruchoir's own apps: public clients (`none`, PKCE), redirected to a loopback address on any port
/// for the desktop and to the app's private-use scheme for the mobile (RFC 8252, sections 7.1 and
/// 7.3). Propolis lets them ask a token for the hub and for every product installed.
fn app(client_id: &str, name: &str, redirect: &str) -> RegistryClient {
    RegistryClient {
        client_id: client_id.to_owned(),
        name: name.to_owned(),
        first_party: true,
        token_endpoint_auth_method: Some("none".to_owned()),
        resource: None,
        redirect_uris: vec![redirect.to_owned()],
        post_logout_redirect_uris: Vec::new(),
        grant_types: strings(&["authorization_code", "refresh_token"]),
        scopes: strings(&[
            "openid",
            "profile",
            "email",
            "organizations",
            "offline_access",
        ]),
        resource_scopes: Vec::new(),
        backchannel_logout_uri: None,
        token_exchange: Vec::new(),
        jwks: Vec::new(),
    }
}

#[derive(Serialize)]
struct RegistryExchange {
    audience: String,
    scopes: Vec<String>,
}

#[derive(Serialize)]
struct RegistryKey {
    kid: String,
    purpose: String,
    kty: String,
    crv: String,
    x: String,
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

/// The scopes the chat declares: posting a card in a channel, and reading a channel and who reads it.
pub const CHAT_SCOPES: [&str; 2] = ["chat:cards.post", "chat:channels.read"];

/// Propolis's client registry.
pub fn registry(instance: &Instance, secrets: &Secrets) -> String {
    let jwks = |service: &str| {
        [("auth", "client_auth"), ("events", "events")]
            .into_iter()
            .map(|(purpose, registered)| {
                let k = secrets.key(service, purpose);
                RegistryKey {
                    kid: k.kid.clone(),
                    purpose: registered.to_owned(),
                    kty: "OKP".to_owned(),
                    crv: "Ed25519".to_owned(),
                    x: k.x.clone(),
                }
            })
            .collect()
    };
    let hub = instance.url("hub");
    let mut client = vec![RegistryClient {
        client_id: "hub".to_owned(),
        name: "Hub".to_owned(),
        first_party: true,
        token_endpoint_auth_method: None,
        resource: Some(hub.clone()),
        redirect_uris: Vec::new(),
        post_logout_redirect_uris: Vec::new(),
        // The hub reads Propolis's change feed in its own name.
        grant_types: strings(&["client_credentials"]),
        // Propolis's change feed for the members' replica, and its mail for the unread digest.
        scopes: strings(&["propolis:organizations.read", "propolis:mail.digest"]),
        // What the products are given to reach the hub.
        resource_scopes: strings(&["hub:publish"]),
        backchannel_logout_uri: Some(format!("{hub}/api/v1/backchannel-logout")),
        token_exchange: Vec::new(),
        jwks: jwks("hub"),
    }];
    if instance.has("drive") {
        let drive = instance.url("drive");
        client.push(RegistryClient {
            client_id: "drive".to_owned(),
            name: "Drive".to_owned(),
            first_party: true,
            token_endpoint_auth_method: None,
            resource: Some(drive.clone()),
            // People sign in to the drive (its own session, `/auth/*`).
            redirect_uris: vec![format!("{drive}/auth/callback")],
            post_logout_redirect_uris: vec![format!("{drive}/")],
            // Its session, kept by refreshing, and its members' replica read in its own name.
            grant_types: strings(&["authorization_code", "refresh_token", "client_credentials"]),
            scopes: strings(&[
                "openid",
                "profile",
                "email",
                "organizations",
                "propolis:organizations.read",
                // Its events, live frames and inbox entries, handed to the hub.
                "hub:publish",
            ]),
            // What another product is given to manage the files it holds in the drive.
            resource_scopes: strings(&["drive:held.manage"]),
            backchannel_logout_uri: Some(format!("{drive}/auth/backchannel-logout")),
            token_exchange: Vec::new(),
            jwks: jwks("drive"),
        });
    }
    if instance.has("calendar") {
        let calendar = instance.url("calendar");
        let mut scopes = vec![
            "openid",
            "profile",
            "email",
            "organizations",
            "propolis:organizations.read",
            // Its live frames, inbox entries and events, handed to the hub.
            "hub:publish",
        ];
        if instance.has("drive") {
            // The files of events it holds in the drive, checked and erased in its own name.
            scopes.push("drive:held.manage");
        }
        if instance.has("chat") {
            // Its cards posted in channels, and who reads a channel asked, in its own name.
            scopes.extend(CHAT_SCOPES);
        }
        client.push(RegistryClient {
            client_id: "calendar".to_owned(),
            name: "Calendar".to_owned(),
            first_party: true,
            token_endpoint_auth_method: None,
            resource: Some(calendar.clone()),
            // People sign in to the calendar (its own session, `/auth/*`), as to the drive.
            redirect_uris: vec![format!("{calendar}/auth/callback")],
            post_logout_redirect_uris: vec![format!("{calendar}/")],
            grant_types: strings(&["authorization_code", "refresh_token", "client_credentials"]),
            scopes: strings(&scopes),
            // The calendar declares no scope of its own: no other service calls its API.
            resource_scopes: Vec::new(),
            backchannel_logout_uri: Some(format!("{calendar}/auth/backchannel-logout")),
            token_exchange: Vec::new(),
            jwks: jwks("calendar"),
        });
    }
    if instance.has("chat") {
        let chat = instance.url("chat");
        client.push(RegistryClient {
            client_id: "chat".to_owned(),
            name: "Chat".to_owned(),
            first_party: true,
            token_endpoint_auth_method: None,
            resource: Some(chat.clone()),
            // People sign in to the chat (its own session, `/auth/*`), as to the drive.
            redirect_uris: vec![format!("{chat}/auth/callback")],
            post_logout_redirect_uris: vec![format!("{chat}/")],
            // Its session, its members' replica read in its own name, and a person's token exchanged
            // for one for the hub, which its pages' socket tickets are asked with.
            grant_types: strings(&[
                "authorization_code",
                "refresh_token",
                "client_credentials",
                "urn:ietf:params:oauth:grant-type:token-exchange",
            ]),
            scopes: strings(&[
                "openid",
                "profile",
                "email",
                "organizations",
                "propolis:organizations.read",
                // Its live frames, inbox entries and events, handed to the hub.
                "hub:publish",
                // The files of messages it holds in the drive (the drive always comes with it).
                "drive:held.manage",
            ]),
            // What another product is given to reach its channels: the calendar, for its cards.
            resource_scopes: strings(&CHAT_SCOPES),
            backchannel_logout_uri: Some(format!("{chat}/auth/backchannel-logout")),
            // The hub's ticket is asked with the person's token, exchanged: no scope is needed there.
            token_exchange: vec![RegistryExchange {
                audience: hub.clone(),
                scopes: Vec::new(),
            }],
            jwks: jwks("chat"),
        });
    }
    if instance.lab {
        let lab = instance.url(crate::LAB);
        client.push(RegistryClient {
            client_id: "ruchoir-lab".to_owned(),
            name: "Ruchoir Lab".to_owned(),
            first_party: true,
            token_endpoint_auth_method: None,
            resource: Some(lab.clone()),
            // The team signs in to Lab (its own session, `/auth/*`), as to a product.
            redirect_uris: vec![format!("{lab}/auth/callback")],
            post_logout_redirect_uris: vec![format!("{lab}/")],
            // Its session, and the structure of the organisation it lets in, read in its own name.
            grant_types: strings(&["authorization_code", "refresh_token", "client_credentials"]),
            scopes: strings(&[
                "openid",
                "profile",
                "email",
                "organizations",
                "propolis:organizations.read",
            ]),
            resource_scopes: Vec::new(),
            backchannel_logout_uri: Some(format!("{lab}/auth/backchannel-logout")),
            token_exchange: Vec::new(),
            jwks: jwks(crate::LAB),
        });
    }
    client.push(app(
        "ruchoir-desktop",
        "Ruchoir (desktop)",
        "http://127.0.0.1/callback",
    ));
    client.push(app(
        "ruchoir-mobile",
        "Ruchoir (mobile)",
        "com.ruchoir.app:/oauth/callback",
    ));
    let registry = Registry { client };
    let header =
        "# The clients of this instance, written by ruchoir-install: never edit by hand, run\n\
                  # `ruchoir-install init` again. Public keys only.\n\n";
    header.to_owned() + &toml::to_string(&registry).expect("a registry serialises")
}

/// The hub's event subscriptions, for the products installed.
pub fn subscriptions(instance: &Instance) -> String {
    let mut text = String::from(
        "# Which service receives which events, written by ruchoir-install, for the products installed.\n",
    );
    let mut subscribe = |service: &str, kinds: &[&str]| {
        let kinds: Vec<String> = kinds.iter().map(|k| format!("\"{k}\"")).collect();
        text.push_str(&format!(
            "\n[[subscription]]\nservice = \"{service}\"\nendpoint = \"{}/api/v1/events\"\nkinds = [{}]\n",
            instance.url(service),
            kinds.join(", ")
        ));
    };
    if instance.has("drive") {
        // The drive copies the organisations and deletes a person's data on Propolis's events.
        subscribe("drive", &["propolis.*"]);
    }
    if instance.has("calendar") {
        // The calendar does the same, and with the drive takes off its events the files the drive
        // erased (`drive.file.purged`).
        if instance.has("drive") {
            subscribe("calendar", &["propolis.*", "drive.file.*"]);
        } else {
            subscribe("calendar", &["propolis.*"]);
        }
    }
    if instance.has("chat") {
        // The chat does the same, and takes off its messages the files the drive erased.
        subscribe("chat", &["propolis.*", "drive.file.*"]);
    }
    text
}

/// The origins of the products' pages: the only pages that may open a hub socket, and the hosts
/// the hub delivers events to.
pub fn page_origins(instance: &Instance) -> String {
    instance
        .products
        .iter()
        .map(|p| instance.url(p))
        .collect::<Vec<_>>()
        .join(",")
}

/// The origins of the other products' pages, which the drive answers with CORS on the files they
/// hold in it (`DRIVE_PRODUCT_ORIGINS`): every product installed but the drive.
pub fn drive_product_origins(instance: &Instance) -> String {
    instance
        .products
        .iter()
        .filter(|p| *p != "drive")
        .map(|p| instance.url(p))
        .collect::<Vec<_>>()
        .join(",")
}

/// One database and one role per service, made the first time PostgreSQL starts.
pub fn postgres_init(instance: &Instance) -> String {
    let mut script = String::from(
        "#!/bin/sh\n# One database and one role per service (ruchoir-install). Runs once, on an empty data directory.\nset -eu\n",
    );
    for service in instance.services() {
        let variable = format!("{}_DB_PASSWORD", service.to_ascii_uppercase());
        script.push_str(&format!(
            "psql -v ON_ERROR_STOP=1 --username \"$POSTGRES_USER\" <<SQL\n\
             CREATE ROLE {service} LOGIN PASSWORD '${variable}';\n\
             CREATE DATABASE {service} OWNER {service};\n\
             SQL\n"
        ));
    }
    script
}

/// `id.example.org` from `https://id.example.org`.
fn host_of(url: &str) -> &str {
    url.trim_start_matches("https://")
}

/// Traefik's static configuration.
pub fn traefik_static(instance: &Instance) -> String {
    let resolver = match (&instance.dev, &instance.acme_email) {
        (false, email) => format!(
            r#"
certificatesResolvers:
  actalis:
    acme:
      caServer: {ACTALIS_ACME}
      email: {}
      storage: /acme/acme.json
      tlsChallenge: {{}}
"#,
            email.as_deref().unwrap_or_default()
        ),
        (true, _) => String::new(),
    };
    format!(
        r#"# Traefik, the proxy (Traefik Labs, France), written by ruchoir-install.
entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
  websecure:
    address: ":443"
    http:
      middlewares: [security@file]

providers:
  file:
    filename: /etc/traefik/dynamic.yml

# Codes and tokens travel in query strings: the access log keeps the path alone, and no header.
accessLog:
  fields:
    names:
      RequestPath: drop
      RequestAddr: drop
    headers:
      defaultMode: drop
{resolver}"#
    )
}

/// Traefik's routes, each service's host to the service, and the headers every answer carries (the
/// services set their own content security policies).
pub fn traefik_dynamic(instance: &Instance) -> String {
    let tls = if instance.dev {
        "      tls: {}\n"
    } else {
        "      tls:\n        certResolver: actalis\n"
    };
    let mut routers = String::new();
    let mut services = String::new();
    for service in instance.services() {
        let host = host_of(&instance.url(service)).to_owned();
        routers.push_str(&format!(
            "    {service}:\n      rule: Host(`{host}`)\n      entryPoints: [websecure]\n      service: {service}\n{tls}"
        ));
        services.push_str(&format!(
            "    {service}:\n      loadBalancer:\n        servers:\n          - url: http://{service}:8080\n"
        ));
    }
    if instance.has("drive") {
        // The office editor's own hostname goes to the drive, which relays it to the engine, its
        // co-editing WebSocket included (Traefik carries upgrades as they come). The `Host` header
        // is passed on unchanged: it is how the drive tells the editor's requests from its own.
        let host = host_of(&crate::office::public_url(&instance.domain)).to_owned();
        let office = crate::office::SERVICE;
        routers.push_str(&format!(
            "    {office}:\n      rule: Host(`{host}`)\n      entryPoints: [websecure]\n      service: drive\n{tls}"
        ));
    }
    // Browsers would remember HSTS for localhost long after the development instance is gone.
    let hsts = if instance.dev {
        ""
    } else {
        "        stsSeconds: 63072000
        stsIncludeSubdomains: true
"
    };
    let certificates = if instance.dev {
        "\ntls:\n  certificates:\n    - certFile: /dev-ca/cert.pem\n      keyFile: /dev-ca/key.pem\n"
    } else {
        ""
    };
    format!(
        r#"# The routes, written by ruchoir-install.
http:
  middlewares:
    security:
      headers:
{hsts}        contentTypeNosniff: true
        referrerPolicy: no-referrer
        customResponseHeaders:
          Server: ""
  routers:
{routers}  services:
{services}{certificates}"#
    )
}

/// The Compose project's name, which prefixes the containers, the network and the volumes: `ruchoir`
/// for a production instance. In development, the directory's name (prefixed with `ruchoir-` unless
/// it starts with it) and a short hash of its full path (`ruchoir-dev-1a2b3c4d`), so two development
/// instances in different directories, even both named `ruchoir-dev`, never share their volumes
/// (the databases of one would answer the other with other passwords).
pub fn project_name(instance: &Instance, dir: &Path) -> String {
    if !instance.dev {
        return "ruchoir".to_owned();
    }
    let path = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let folder = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "{}-{:08x}",
        project_of_folder(&folder),
        fnv1a(path.to_string_lossy().as_bytes())
    )
}

/// FNV-1a on 32 bits: stable across runs and versions, unlike the standard library's hasher.
fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

/// A Compose project name from a directory's name: lower case letters, digits, `-` and `_` only.
pub fn project_of_folder(folder: &str) -> String {
    let name: String = folder
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches(|c: char| c == '-' || c == '_');
    if name.is_empty() {
        "ruchoir-dev".to_owned()
    } else if name.starts_with("ruchoir") {
        name.to_owned()
    } else {
        format!("ruchoir-{name}")
    }
}

/// In development, the hub wakes no phone: the publisher's push relay is for real instances.
fn push_relay_of(instance: &Instance) -> &'static str {
    if instance.dev {
        "      HUB_PUSH_RELAY_URL: \"\"\n"
    } else {
        ""
    }
}

/// The compose file, for the Compose project `project` ([`project_name`]).
pub fn compose(instance: &Instance, secrets: &Secrets, project: &str) -> String {
    let push_relay = push_relay_of(instance);
    let hub_kid = &secrets.key("hub", "auth").kid;
    let page_origins = page_origins(instance);
    let propolis = instance.url("propolis");
    let hub = instance.url("hub");
    let aliases: String = instance
        .services()
        .into_iter()
        .map(|s| format!("          - {}\n", host_of(&instance.url(s))))
        .collect();
    let (drive_volumes, mut products_db, drive) = if instance.has("drive") {
        (
            "  garage-meta:\n  garage-data:\n",
            "      DRIVE_DB_PASSWORD: ${DRIVE_DB_PASSWORD}\n".to_owned(),
            drive_services(instance, secrets),
        )
    } else {
        ("", String::new(), String::new())
    };
    let calendar = if instance.has("calendar") {
        products_db.push_str("      CALENDAR_DB_PASSWORD: ${CALENDAR_DB_PASSWORD}\n");
        calendar_service(instance, secrets)
    } else {
        String::new()
    };
    let chat = if instance.has("chat") {
        products_db.push_str("      CHAT_DB_PASSWORD: ${CHAT_DB_PASSWORD}\n");
        chat_service(instance, secrets)
    } else {
        String::new()
    };
    let lab_volumes = if instance.lab {
        "  lab-captures:\n"
    } else {
        ""
    };
    let lab = if instance.lab {
        products_db.push_str("      LAB_DB_PASSWORD: ${LAB_DB_PASSWORD}\n");
        lab_service(instance, secrets)
    } else {
        String::new()
    };
    let office_network = if instance.has("drive") {
        "  # The office engine's network: the engine and the drive, no route out.\n  office:\n    internal: true\n"
    } else {
        ""
    };
    let (propolis_image, hub_image) = if instance.dev {
        // Built from the repositories (`ruchoir-install dev` says how).
        ("ruchoir/propolis:dev", "ruchoir/hub:dev")
    } else {
        (
            "registry.ruchoir.com/propolis:latest",
            "registry.ruchoir.com/hub:latest",
        )
    };
    // In development the services reach each other's public names through the proxy and trust the
    // development authority; mails go to the catcher.
    let (trust, ca_volume) = (trust_of(instance), ca_volume_of(instance));
    let (mail, catcher) = if instance.dev {
        (
            r#"      PROPOLIS_SMTP_HOST: mailpit
      PROPOLIS_SMTP_PORT: "1025"
      PROPOLIS_SMTP_TLS: none
      PROPOLIS_SMTP_FROM: Ruchoir <no-reply@localhost>
"#,
            r#"  # Catches every mail in development, read at http://localhost:8025 (Mailpit, MIT).
  mailpit:
    image: axllent/mailpit:v1.33.1
    networks: [ruchoir]
    ports:
      - "127.0.0.1:8025:8025"
    restart: unless-stopped

"#,
        )
    } else {
        ("", "")
    };

    format!(
        r#"# A Ruchoir instance, written by ruchoir-install: run `ruchoir-install init` again rather than
# editing it, and put your own changes in compose.override.yaml, which it never touches. The secrets
# are in .env.
name: {project}

networks:
  ruchoir:
    ipam:
      config:
        - subnet: {SUBNET}
{office_network}
volumes:
  postgres-data:
  valkey-data:
  acme:
{drive_volumes}{lab_volumes}
services:
  postgres:
    image: postgres:18-alpine
    environment:
      POSTGRES_USER: ruchoir
      POSTGRES_PASSWORD: ${{POSTGRES_PASSWORD}}
      PROPOLIS_DB_PASSWORD: ${{PROPOLIS_DB_PASSWORD}}
      HUB_DB_PASSWORD: ${{HUB_DB_PASSWORD}}
{products_db}    volumes:
      - postgres-data:/var/lib/postgresql
      - ./postgres/init.sh:/docker-entrypoint-initdb.d/init.sh:ro
    networks: [ruchoir]
    # Over TCP: while the image runs init.sh on its first start, PostgreSQL listens on its socket
    # alone and stops again, and a check on the socket would let the services start too early.
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -h 127.0.0.1 -U ruchoir"]
      interval: 5s
      retries: 20
    restart: unless-stopped

  valkey:
    image: valkey/valkey:9-alpine
    command: ["valkey-server", "--appendonly", "yes"]
    volumes:
      - valkey-data:/data
    networks: [ruchoir]
    healthcheck:
      test: ["CMD", "valkey-cli", "ping"]
      interval: 5s
      retries: 20
    restart: unless-stopped

  traefik:
    image: traefik:v3.7.17
    command: ["--configFile=/etc/traefik/traefik.yml"]
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./traefik:/etc/traefik:ro
      - acme:/acme
{ca_volume}    networks:
      ruchoir:
        # The services reach each other at their public names, through the proxy.
        aliases:
{aliases}    restart: unless-stopped

{catcher}  propolis:
    image: ${{PROPOLIS_IMAGE:-{propolis_image}}}
    # The owner of this directory, who alone reads its private files.
    user: ${{RUCHOIR_USER:-10001:10001}}
    environment:
      PROPOLIS_PUBLIC_URL: {propolis}
      PROPOLIS_DATABASE_URL: postgres://propolis:${{PROPOLIS_DB_PASSWORD}}@postgres:5432/propolis
      PROPOLIS_VALKEY_URL: redis://valkey:6379/0
      PROPOLIS_SECRET_KEY: ${{PROPOLIS_SECRET_KEY}}
      PROPOLIS_TRUSTED_PROXIES: {SUBNET}
      PROPOLIS_CLIENT_REGISTRY: /config/clients.toml
      PROPOLIS_HUB_URL: {hub}
{mail}{trust}    volumes:
      - ./propolis:/config:ro
{ca_volume}    networks: [ruchoir]
    depends_on:
      postgres: {{ condition: service_healthy }}
      valkey: {{ condition: service_healthy }}
    restart: unless-stopped

  hub:
    image: ${{HUB_IMAGE:-{hub_image}}}
    user: ${{RUCHOIR_USER:-10001:10001}}
    environment:
      HUB_PUBLIC_URL: {hub}
      HUB_DATABASE_URL: postgres://hub:${{HUB_DB_PASSWORD}}@postgres:5432/hub
      HUB_VALKEY_URL: redis://valkey:6379/1
      HUB_TRUSTED_PROXIES: {SUBNET}
      HUB_ISSUER: {propolis}
      HUB_CLIENT_ID: hub
      HUB_CLIENT_KEY: /keys/hub-auth.pem
      HUB_CLIENT_KEY_ID: {hub_kid}
      HUB_SUBSCRIPTIONS: /config/subscriptions.toml
      HUB_PAGE_ORIGINS: {page_origins}
      HUB_VAPID_KEY: /keys/hub-vapid.pem
      HUB_MANIFEST: /manifest/manifest.json
{push_relay}{trust}    volumes:
      - ./hub:/config:ro
      - ./manifest.json:/manifest/manifest.json:ro
      - ./keys/hub-auth.pem:/keys/hub-auth.pem:ro
      - ./keys/hub-vapid.pem:/keys/hub-vapid.pem:ro
{ca_volume}    networks: [ruchoir]
    depends_on:
      postgres: {{ condition: service_healthy }}
      valkey: {{ condition: service_healthy }}
      propolis: {{ condition: service_started }}
    restart: unless-stopped
{drive}{calendar}{chat}{lab}"#
    )
}

/// In development, the services trust the development authority.
fn trust_of(instance: &Instance) -> &'static str {
    if instance.dev {
        "      SSL_CERT_FILE: /dev-ca/ca.pem\n"
    } else {
        ""
    }
}

/// In development, the development authority, mounted.
fn ca_volume_of(instance: &Instance) -> &'static str {
    if instance.dev {
        "      - ./dev-ca:/dev-ca:ro\n"
    } else {
        ""
    }
}

/// The drive, its object store Garage and its office engine Euro-Office, none published: the drive
/// is reached through the proxy, Garage and the engine by the drive alone. The S3 key and Garage's
/// secrets come from `.env`.
fn drive_services(instance: &Instance, secrets: &Secrets) -> String {
    let drive = instance.url("drive");
    let propolis = instance.url("propolis");
    let kid = &secrets.key("drive", "auth").kid;
    let events_kid = &secrets.key("drive", "events").kid;
    let hub = instance.url("hub");
    let (trust, ca_volume) = (trust_of(instance), ca_volume_of(instance));
    let image = if instance.dev {
        "ruchoir/drive:dev"
    } else {
        "registry.ruchoir.com/drive:latest"
    };
    let garage = crate::garage::IMAGE;
    let bucket = crate::garage::BUCKET;
    let office_env = crate::office::drive_env(&instance.domain);
    // The other products' pages, which upload and read the files they hold in the drive.
    let origins = drive_product_origins(instance);
    let product_origins = if origins.is_empty() {
        String::new()
    } else {
        format!("      DRIVE_PRODUCT_ORIGINS: {origins}\n")
    };
    let office = crate::office::service();
    format!(
        r#"
  drive:
    image: ${{DRIVE_IMAGE:-{image}}}
    user: ${{RUCHOIR_USER:-10001:10001}}
    environment:
      DRIVE_PUBLIC_URL: {drive}
      DRIVE_DATABASE_URL: postgres://drive:${{DRIVE_DB_PASSWORD}}@postgres:5432/drive
      DRIVE_VALKEY_URL: redis://valkey:6379/2
      DRIVE_TRUSTED_PROXIES: {SUBNET}
      DRIVE_ISSUER: {propolis}
      DRIVE_CLIENT_ID: drive
      DRIVE_CLIENT_KEY: /keys/drive-auth.pem
      DRIVE_CLIENT_KEY_ID: {kid}
      DRIVE_S3_ENDPOINT: http://garage:3900
      DRIVE_S3_REGION: garage
      DRIVE_S3_BUCKET: {bucket}
      DRIVE_S3_ACCESS_KEY_ID: ${{DRIVE_S3_ACCESS_KEY_ID}}
      DRIVE_S3_SECRET_ACCESS_KEY: ${{DRIVE_S3_SECRET_ACCESS_KEY}}
      DRIVE_HUB_URL: {hub}
      DRIVE_EVENTS_KEY: /keys/drive-events.pem
      DRIVE_EVENTS_KEY_ID: {events_kid}
{office_env}{product_origins}{trust}    volumes:
      - ./keys/drive-auth.pem:/keys/drive-auth.pem:ro
      - ./keys/drive-events.pem:/keys/drive-events.pem:ro
{ca_volume}    # The instance's network, and the office engine's, which only the drive shares with it.
    networks: [ruchoir, office]
    depends_on:
      postgres: {{ condition: service_healthy }}
      valkey: {{ condition: service_healthy }}
      propolis: {{ condition: service_started }}
      garage: {{ condition: service_started }}
    restart: unless-stopped

  # The drive's object store (Garage, Deuxfleurs, France; AGPL-3.0), one node, reached at
  # http://garage:3900 on this network alone. `ruchoir-install up` prepares it: layout, key, bucket.
  garage:
    image: {garage}
    environment:
      GARAGE_RPC_SECRET: ${{GARAGE_RPC_SECRET}}
      GARAGE_ADMIN_TOKEN: ${{GARAGE_ADMIN_TOKEN}}
    volumes:
      - ./garage/garage.toml:/etc/garage.toml:ro
      - garage-meta:/var/lib/garage/meta
      - garage-data:/var/lib/garage/data
    networks: [ruchoir]
    # Garage reads and writes its two volumes alone, and needs no capability.
    security_opt: [no-new-privileges:true]
    cap_drop: [ALL]
    read_only: true
    restart: unless-stopped
{office}"#
    )
}

/// The calendar, reached through the proxy alone. Its database password and encryption key come
/// from `.env`. It mails people invited by address: in development through the catcher, in
/// production through the relay an operator sets in `compose.override.yaml` (`CALENDAR_SMTP_*`),
/// and without one the mails are logged, never sent.
fn calendar_service(instance: &Instance, secrets: &Secrets) -> String {
    let calendar = instance.url("calendar");
    let propolis = instance.url("propolis");
    let hub = instance.url("hub");
    let kid = &secrets.key("calendar", "auth").kid;
    let events_kid = &secrets.key("calendar", "events").kid;
    let (trust, ca_volume) = (trust_of(instance), ca_volume_of(instance));
    let image = if instance.dev {
        "ruchoir/calendar:dev"
    } else {
        "registry.ruchoir.com/calendar:latest"
    };
    // With the drive, the files of events are kept there.
    let drive = if instance.has("drive") {
        format!("      CALENDAR_DRIVE_URL: {}\n", instance.url("drive"))
    } else {
        String::new()
    };
    // With the chat, events are posted as cards in its channels.
    let chat = if instance.has("chat") {
        format!("      CALENDAR_CHAT_URL: {}\n", instance.url("chat"))
    } else {
        String::new()
    };
    let mail = if instance.dev {
        r#"      CALENDAR_SMTP_HOST: mailpit
      CALENDAR_SMTP_PORT: "1025"
      CALENDAR_SMTP_TLS: none
      CALENDAR_SMTP_FROM: Ruchoir <no-reply@localhost>
"#
    } else {
        ""
    };
    format!(
        r#"
  calendar:
    image: ${{CALENDAR_IMAGE:-{image}}}
    user: ${{RUCHOIR_USER:-10001:10001}}
    environment:
      CALENDAR_PUBLIC_URL: {calendar}
      CALENDAR_DATABASE_URL: postgres://calendar:${{CALENDAR_DB_PASSWORD}}@postgres:5432/calendar
      CALENDAR_VALKEY_URL: redis://valkey:6379/3
      CALENDAR_TRUSTED_PROXIES: {SUBNET}
      CALENDAR_ISSUER: {propolis}
      CALENDAR_CLIENT_ID: calendar
      CALENDAR_CLIENT_KEY: /keys/calendar-auth.pem
      CALENDAR_CLIENT_KEY_ID: {kid}
      CALENDAR_HUB_URL: {hub}
      CALENDAR_EVENTS_KEY: /keys/calendar-events.pem
      CALENDAR_EVENTS_KEY_ID: {events_kid}
      CALENDAR_SECRET_KEY: ${{CALENDAR_SECRET_KEY}}
{drive}{chat}{mail}{trust}    volumes:
      - ./keys/calendar-auth.pem:/keys/calendar-auth.pem:ro
      - ./keys/calendar-events.pem:/keys/calendar-events.pem:ro
{ca_volume}    networks: [ruchoir]
    depends_on:
      postgres: {{ condition: service_healthy }}
      valkey: {{ condition: service_healthy }}
      propolis: {{ condition: service_started }}
    restart: unless-stopped
"#
    )
}

/// The chat, reached through the proxy alone, with the drive it keeps the files of messages in. Its
/// database password comes from `.env`. It sends no mail. It reads the first link of a message for
/// its preview, never on its own domain, which covers every host of the instance.
fn chat_service(instance: &Instance, secrets: &Secrets) -> String {
    let chat = instance.url("chat");
    let propolis = instance.url("propolis");
    let hub = instance.url("hub");
    let drive = instance.url("drive");
    let kid = &secrets.key("chat", "auth").kid;
    let events_kid = &secrets.key("chat", "events").kid;
    let (trust, ca_volume) = (trust_of(instance), ca_volume_of(instance));
    let image = if instance.dev {
        "ruchoir/chat:dev"
    } else {
        "registry.ruchoir.com/chat:latest"
    };
    format!(
        r#"
  chat:
    image: ${{CHAT_IMAGE:-{image}}}
    user: ${{RUCHOIR_USER:-10001:10001}}
    environment:
      CHAT_PUBLIC_URL: {chat}
      CHAT_DATABASE_URL: postgres://chat:${{CHAT_DB_PASSWORD}}@postgres:5432/chat
      CHAT_VALKEY_URL: redis://valkey:6379/4
      CHAT_TRUSTED_PROXIES: {SUBNET}
      CHAT_ISSUER: {propolis}
      CHAT_CLIENT_ID: chat
      CHAT_CLIENT_KEY: /keys/chat-auth.pem
      CHAT_CLIENT_KEY_ID: {kid}
      CHAT_HUB_URL: {hub}
      CHAT_EVENTS_KEY: /keys/chat-events.pem
      CHAT_EVENTS_KEY_ID: {events_kid}
      CHAT_DRIVE_URL: {drive}
      CHAT_UNFURL_ENABLED: "true"
{trust}    volumes:
      - ./keys/chat-auth.pem:/keys/chat-auth.pem:ro
      - ./keys/chat-events.pem:/keys/chat-events.pem:ro
{ca_volume}    networks: [ruchoir]
    depends_on:
      postgres: {{ condition: service_healthy }}
      valkey: {{ condition: service_healthy }}
      propolis: {{ condition: service_started }}
      drive: {{ condition: service_started }}
    restart: unless-stopped
"#
    )
}

/// Lab, the team's tool, reached through the proxy alone, on a development instance, and its runner
/// (the browser tests and the servers' status), which writes the screenshots Lab reads. Its database
/// password comes from `.env`. It watches the instance it runs beside: the products of the hub's
/// manifest (`LAB_HUB_URL`). `LAB_ORGANISATION`, the organisation whose members get in, is set by
/// hand once it exists in Propolis (`compose.override.yaml`, or the shell): until then nobody gets in.
fn lab_service(instance: &Instance, secrets: &Secrets) -> String {
    let lab = instance.url(crate::LAB);
    let propolis = instance.url("propolis");
    let hub = instance.url("hub");
    let kid = &secrets.key(crate::LAB, "auth").kid;
    let (trust, ca_volume) = (trust_of(instance), ca_volume_of(instance));
    format!(
        r#"
  lab:
    image: ${{LAB_IMAGE:-ruchoir/lab:dev}}
    user: ${{RUCHOIR_USER:-10001:10001}}
    environment:
      LAB_PUBLIC_URL: {lab}
      LAB_DATABASE_URL: postgres://lab:${{LAB_DB_PASSWORD}}@postgres:5432/lab
      LAB_VALKEY_URL: redis://valkey:6379/5
      LAB_TRUSTED_PROXIES: {SUBNET}
      LAB_ISSUER: {propolis}
      LAB_CLIENT_ID: ruchoir-lab
      LAB_CLIENT_KEY: /keys/lab-auth.pem
      LAB_CLIENT_KEY_ID: {kid}
      LAB_ORGANISATION: ${{LAB_ORGANISATION:-}}
      LAB_HUB_URL: {hub}
      LAB_DATA_DIR: /srv/lab/data
{trust}    volumes:
      - ./keys/lab-auth.pem:/keys/lab-auth.pem:ro
      - lab-captures:/srv/lab/data:ro
{ca_volume}    networks: [ruchoir]
    depends_on:
      postgres: {{ condition: service_healthy }}
      valkey: {{ condition: service_healthy }}
      propolis: {{ condition: service_started }}
    restart: unless-stopped

  lab-runner:
    image: ${{LAB_RUNNER_IMAGE:-ruchoir/lab-runner:dev}}
    environment:
      LAB_DATABASE_URL: postgres://lab:${{LAB_DB_PASSWORD}}@postgres:5432/lab
      NODE_EXTRA_CA_CERTS: /dev-ca/ca.pem
    volumes:
      - lab-captures:/srv/lab/data
      - ./dev-ca:/dev-ca:ro
    # In the proxy's network namespace: Chromium sends every *.localhost name to the loopback, where
    # the proxy listens. The image adds the development authority to Chromium's store at start.
    network_mode: service:traefik
    depends_on:
      postgres: {{ condition: service_healthy }}
      lab: {{ condition: service_started }}
    init: true
    shm_size: 1gb
    restart: unless-stopped
"#
    )
}

/// Write every file but the secrets.
pub fn write_all(dir: &Path, instance: &Instance, secrets: &Secrets) -> Result<(), String> {
    write(dir, "manifest.json", &manifest(instance))?;
    write(dir, "propolis/clients.toml", &registry(instance, secrets))?;
    write(dir, "hub/subscriptions.toml", &subscriptions(instance))?;
    write(dir, "postgres/init.sh", &postgres_init(instance))?;
    if instance.has("drive") {
        write(dir, "garage/garage.toml", &crate::garage::config())?;
        for file in crate::office::FILES {
            write_bytes(dir, file.path, file.bytes)?;
        }
    }
    write(dir, "traefik/traefik.yml", &traefik_static(instance))?;
    write(dir, "traefik/dynamic.yml", &traefik_dynamic(instance))?;
    write(
        dir,
        "compose.yaml",
        &compose(instance, secrets, &project_name(instance, dir)),
    )?;
    Ok(())
}
