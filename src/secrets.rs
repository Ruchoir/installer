//! What is generated once for an instance and kept: running `init` again never replaces a secret or a
//! key, since the services and their data depend on them.
//!
//! - `.env`: the database passwords and Propolis's encryption key (64 hexadecimal characters).
//! - `keys/<service>-auth.pem` and `keys/<service>-events.pem`: each client service's Ed25519 keys, for
//!   `private_key_jwt` and for signing its events. A private key goes to its own service alone;
//!   Propolis's registry holds the public halves.
//! - `keys/hub-vapid.pem`: the instance's VAPID key, which browsers subscribe against for web push.
//! - With the drive, in `.env`: its database password, Garage's RPC secret and admin token, and the
//!   drive's S3 key, which the installer makes and Garage is told to accept (see [`crate::garage`]).
//!   They are made when the drive is installed, and kept like the others.

use std::collections::BTreeMap;
use std::path::Path;

use base64::Engine as _;
use jwt_simple::prelude::Ed25519KeyPair;

use crate::Instance;

/// The variables of `.env`, in the order they are written.
pub const VARIABLES: [&str; 4] = [
    "POSTGRES_PASSWORD",
    "PROPOLIS_DB_PASSWORD",
    "HUB_DB_PASSWORD",
    "PROPOLIS_SECRET_KEY",
];

/// The variables of `.env` the drive adds, in the order they are written.
pub const DRIVE_VARIABLES: [&str; 5] = [
    "DRIVE_DB_PASSWORD",
    "GARAGE_RPC_SECRET",
    "GARAGE_ADMIN_TOKEN",
    "DRIVE_S3_ACCESS_KEY_ID",
    "DRIVE_S3_SECRET_ACCESS_KEY",
];

/// A new value for the variable `name` of `.env`.
fn make(name: &str) -> Result<String, String> {
    match name {
        // Garage's own key ids are `GK` and 24 hexadecimal characters: the drive's key looks like one.
        "DRIVE_S3_ACCESS_KEY_ID" => Ok(format!("GK{}", random_hex(12)?)),
        // 32 bytes in hexadecimal: Garage wants its RPC secret in exactly this form.
        "PROPOLIS_SECRET_KEY"
        | "GARAGE_RPC_SECRET"
        | "GARAGE_ADMIN_TOKEN"
        | "DRIVE_S3_SECRET_ACCESS_KEY" => random_hex(32),
        _ => random_hex(24),
    }
}

/// The user the services run as, `uid:gid`: the owner of the instance's directory.
pub const USER: &str = "RUCHOIR_USER";

/// The owner of `dir`, as `uid:gid`. On Windows and macOS, Docker's file sharing reads any file, and
/// the images keep their own user.
fn owner(dir: &Path) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(dir).ok()?;
        Some(format!("{}:{}", meta.uid(), meta.gid()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = dir;
        None
    }
}

/// One Ed25519 key of a service.
#[derive(Debug, Clone)]
pub struct ServiceKey {
    /// The key id the registry and the service name it by.
    pub kid: String,
    /// The public half, base64url, as a JWK's `x`.
    pub x: String,
}

/// The instance's secrets and the public halves of its keys.
#[derive(Debug, Clone)]
pub struct Secrets {
    pub env: BTreeMap<String, String>,
    /// `(service, purpose)`: `purpose` is `auth` or `events`.
    pub keys: BTreeMap<(String, String), ServiceKey>,
}

fn random_hex(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|e| format!("no randomness: {e}"))?;
    Ok(buffer.iter().map(|b| format!("{b:02x}")).collect())
}

/// Read `.env` as `NAME=value` lines.
pub fn read_env(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

/// A service's key of `purpose`, made once: the PEM file is kept, its id beside it.
fn key(dir: &Path, service: &str, purpose: &str) -> Result<ServiceKey, String> {
    let keys = dir.join("keys");
    std::fs::create_dir_all(&keys).map_err(|e| format!("{}: {e}", keys.display()))?;
    let pem_path = keys.join(format!("{service}-{purpose}.pem"));
    let kid_path = keys.join(format!("{service}-{purpose}.kid"));
    let pair = match std::fs::read_to_string(&pem_path) {
        Ok(pem) => {
            Ed25519KeyPair::from_pem(&pem).map_err(|e| format!("{}: {e}", pem_path.display()))?
        }
        Err(_) => {
            let pair = Ed25519KeyPair::generate();
            write_private(&pem_path, &pair.to_pem())?;
            let kid = format!("{service}-{purpose}-{}", &random_hex(6)?);
            std::fs::write(&kid_path, &kid).map_err(|e| format!("{}: {e}", kid_path.display()))?;
            pair
        }
    };
    let kid = std::fs::read_to_string(&kid_path)
        .map_err(|e| format!("{}: {e}", kid_path.display()))?
        .trim()
        .to_owned();
    Ok(ServiceKey {
        kid,
        x: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(pair.public_key().to_bytes()),
    })
}

/// The instance's VAPID key (RFC 8292), made once: `keys/hub-vapid.pem`, a P-256 private key in
/// PKCS#8. Browsers subscribe against its public half, so replacing it would drop every browser's
/// subscription.
fn vapid(dir: &Path) -> Result<(), String> {
    let path = dir.join("keys").join("hub-vapid.pem");
    if path.exists() {
        return Ok(());
    }
    let pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(|e| format!("VAPID key: {e}"))?;
    write_private(&path, &pair.serialize_pem())
}

/// Write a file only its owner reads.
pub fn write_private(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

impl Secrets {
    /// The secrets of `instance` in `dir`, making what is missing and keeping what is there. A
    /// product's secrets are made when it is installed.
    pub fn load_or_make(dir: &Path, instance: &Instance) -> Result<Self, String> {
        let env_path = dir.join(".env");
        let mut env = read_env(&env_path);
        let mut changed = false;
        let drive: &[&str] = if instance.has("drive") {
            &DRIVE_VARIABLES
        } else {
            &[]
        };
        for name in VARIABLES.iter().chain(drive) {
            if env.get(*name).is_none_or(|v| v.is_empty()) {
                env.insert((*name).to_owned(), make(name)?);
                changed = true;
            }
        }
        // The services run as the directory's owner, who alone reads its private files.
        if let Some(owner) = owner(dir) {
            if env.get(USER).map(String::as_str) != Some(owner.as_str()) {
                env.insert(USER.to_owned(), owner);
                changed = true;
            }
        }
        if changed {
            let known = |k: &str| VARIABLES.contains(&k) || DRIVE_VARIABLES.contains(&k);
            let text: String = VARIABLES
                .iter()
                .chain(&DRIVE_VARIABLES)
                .copied()
                .filter(|k| env.contains_key(*k))
                .chain(env.keys().map(String::as_str).filter(|k| !known(k)))
                .map(|name| format!("{name}={}\n", env[name]))
                .collect();
            write_private(&env_path, &text)?;
        }
        let mut keys = BTreeMap::new();
        // Propolis keeps its own keys, in its database: the registry is for its clients.
        for service in instance.services().into_iter().filter(|s| *s != "propolis") {
            for purpose in ["auth", "events"] {
                keys.insert(
                    (service.to_owned(), purpose.to_owned()),
                    key(dir, service, purpose)?,
                );
            }
        }
        vapid(dir)?;
        Ok(Self { env, keys })
    }

    /// A service's key of `purpose`.
    pub fn key(&self, service: &str, purpose: &str) -> &ServiceKey {
        &self.keys[&((*service).to_owned(), purpose.to_owned())]
    }
}
