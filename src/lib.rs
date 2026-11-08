//! `ruchoir-install`: what an instance is made of, written to one directory (plan of the installer v0,
//! ADR 0004 and 0012 of `Ruchoir/.github`).
//!
//! - [`Instance`]: the domain, the mode (production or development) and the services installed.
//!   Propolis and the hub are always there; the products come with their phases.
//! - [`secrets`]: what is generated once and kept (database passwords, Propolis's encryption key,
//!   each service's Ed25519 keys). Running `init` again keeps them.
//! - [`render`]: the files the services and the proxy read, from the instance and its secrets.
//! - [`dev_ca`]: in development, a local certificate authority and a certificate for `*.localhost`,
//!   which the proxy serves and the services trust.

pub mod dev_ca;
pub mod render;
pub mod secrets;

use std::path::Path;

/// The services every instance has.
pub const ALWAYS: [&str; 2] = ["propolis", "hub"];

/// An instance to set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// The domain its services live under: `id.<domain>` for Propolis (ADR 0012), `hub.<domain>`.
    pub domain: String,
    /// Development: `localhost`, a local certificate, a mail catcher, images built from source.
    pub dev: bool,
    /// The address certificates are asked for (ACME), in production.
    pub acme_email: Option<String>,
}

impl Instance {
    /// A production instance under `domain`.
    pub fn production(domain: &str, acme_email: Option<String>) -> Result<Self, String> {
        let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
        let label = |label: &str| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        };
        let valid = domain.contains('.') && domain.len() <= 253 && domain.split('.').all(label);
        if !valid {
            return Err(format!("{domain:?} is not a domain name"));
        }
        Ok(Self {
            domain,
            dev: false,
            acme_email,
        })
    }

    /// The development instance, on `localhost`.
    pub fn development() -> Self {
        Self {
            domain: "localhost".to_owned(),
            dev: true,
            acme_email: None,
        }
    }

    /// The public address of a service.
    pub fn url(&self, service: &str) -> String {
        let host = match service {
            "propolis" => "id",
            other => other,
        };
        format!("https://{host}.{}", self.domain)
    }
}

/// Write the instance's directory: secrets kept, every other file written again.
pub fn init(dir: &Path, instance: &Instance) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let secrets = secrets::Secrets::load_or_make(dir)?;
    if instance.dev {
        dev_ca::write(dir, &instance.domain)?;
    }
    render::write_all(dir, instance, &secrets)
}
