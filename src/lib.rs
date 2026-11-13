//! `ruchoir-install`: what an instance is made of, written to one directory (ADR 0004 and 0012 of
//! `Ruchoir/.github`).
//!
//! - [`Instance`]: the domain, the mode (production or development) and the services installed.
//!   Propolis and the hub are always there; the products are chosen ([`PRODUCTS`]).
//! - [`secrets`]: what is generated once and kept (database passwords, Propolis's encryption key,
//!   each service's Ed25519 keys, Garage's secrets and the drive's S3 key, the calendar's
//!   encryption key, the chat's database password). Running `init` again keeps them.
//! - [`render`]: the files the services and the proxy read, from the instance and its secrets.
//! - [`garage`]: the object store the drive keeps its files in: its configuration, and the commands
//!   that prepare a fresh one.
//! - [`office`]: Euro-Office, the office engine that always comes with the drive: its service, its
//!   files, and the drive's settings for it.
//! - [`dev_ca`]: in development, a local certificate authority and a certificate for `*.localhost`,
//!   which the proxy serves and the services trust.
//! - Lab, the team's internal tool ([`LAB`]): in development only, never in the manifest, kept once
//!   installed (its client key is there, [`lab_kept`]).
//!
//! The products installed are those of the manifest written last: `init --products drive` adds the
//! drive, and a later `init` without `--products` keeps what the manifest lists
//! ([`products_kept`]). A product is never removed by `init`: its data would be left without a
//! service.

pub mod dev_ca;
pub mod garage;
pub mod hosts;
pub mod migrate;
pub mod office;
pub mod render;
pub mod secrets;

use std::path::Path;

/// The services every instance has.
pub const ALWAYS: [&str; 2] = ["propolis", "hub"];

/// The products that can be installed besides Propolis and the hub, each on its own or with the
/// others. The calendar works alone; with the drive, its events carry files the drive keeps. The
/// chat needs the drive ([`REQUIRES`]); with the calendar, events are posted as cards in channels.
pub const PRODUCTS: [&str; 3] = ["drive", "calendar", "chat"];

/// The products another one cannot go without, installed with it: the chat's attachments are files
/// of the drive, so choosing the chat installs the drive too.
pub const REQUIRES: [(&str, &str); 1] = [("chat", "drive")];

/// Lab, the team's internal tool: a service of a development instance when asked (`dev --lab`), never
/// a product of the manifest nor offered to customers.
pub const LAB: &str = "lab";

/// An instance to set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// The domain its services live under: `id.<domain>` for Propolis (ADR 0012), `hub.<domain>`,
    /// `drive.<domain>`, `calendar.<domain>`, `chat.<domain>`.
    pub domain: String,
    /// Development: `localhost`, a local certificate, a mail catcher, images built from source.
    pub dev: bool,
    /// The address certificates are asked for (ACME), in production.
    pub acme_email: Option<String>,
    /// The products installed, in the order of [`PRODUCTS`].
    pub products: Vec<String>,
    /// Whether Lab runs beside them (development only).
    pub lab: bool,
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
            products: Vec::new(),
            lab: false,
        })
    }

    /// The development instance, on `localhost`.
    pub fn development() -> Self {
        Self {
            domain: "localhost".to_owned(),
            dev: true,
            acme_email: None,
            products: Vec::new(),
            lab: false,
        }
    }

    /// The same instance with Lab beside the products, or without it. Only a development instance
    /// runs Lab.
    pub fn with_lab(mut self, lab: bool) -> Result<Self, String> {
        if lab && !self.dev {
            return Err("Lab runs on a development instance only".to_owned());
        }
        self.lab = lab;
        Ok(self)
    }

    /// The same instance with `products` installed. A name that is not a product is refused.
    pub fn with_products<S: AsRef<str>>(mut self, products: &[S]) -> Result<Self, String> {
        self.products = checked_products(products)?;
        Ok(self)
    }

    /// Whether `product` is installed.
    pub fn has(&self, product: &str) -> bool {
        self.products.iter().any(|p| p == product)
    }

    /// Every service of the instance: Propolis, the hub, the products, then Lab when it runs.
    pub fn services(&self) -> Vec<&str> {
        let mut services = self.listed();
        if self.lab {
            services.push(LAB);
        }
        services
    }

    /// The services the manifest lists: Propolis, the hub, then the products. Never Lab.
    pub fn listed(&self) -> Vec<&str> {
        ALWAYS
            .iter()
            .copied()
            .chain(self.products.iter().map(String::as_str))
            .collect()
    }

    /// Every name given a public host, `<name>.<domain>` (`id` for Propolis): the services, and with
    /// the drive the office editor's own hostname, which the proxy routes to the drive.
    pub fn hosted(&self) -> Vec<&str> {
        let mut names = self.services();
        if self.has("drive") {
            names.push(office::SERVICE);
        }
        names
    }

    /// The host name of each of [`Instance::hosted`] (`id.<domain>` for Propolis).
    pub fn host_names(&self) -> Vec<String> {
        self.hosted()
            .into_iter()
            .map(|s| self.url(s).trim_start_matches("https://").to_owned())
            .collect()
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

/// `products` checked and put in the order of [`PRODUCTS`], each once, with those they require
/// ([`REQUIRES`]).
fn checked_products<S: AsRef<str>>(products: &[S]) -> Result<Vec<String>, String> {
    let mut asked = Vec::new();
    for product in products {
        let name = product.as_ref().trim().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if !PRODUCTS.contains(&name.as_str()) {
            return Err(format!(
                "{name:?} is not a product: the products are {}",
                PRODUCTS.join(", ")
            ));
        }
        // Choosing the chat installs the drive: its messages carry files the drive keeps.
        for (product, required) in REQUIRES {
            if name == product {
                asked.push(required.to_owned());
            }
        }
        asked.push(name);
    }
    Ok(PRODUCTS
        .iter()
        .filter(|p| asked.iter().any(|a| a == *p))
        .map(|p| (*p).to_owned())
        .collect())
}

/// The products the manifest in `dir` lists, none when there is no manifest yet.
pub fn installed_products(dir: &Path) -> Result<Vec<String>, String> {
    let path = dir.join("manifest.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let manifest: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let services: Vec<&str> = manifest["services"]
        .as_object()
        .map(|services| services.keys().map(String::as_str).collect())
        .unwrap_or_default();
    checked_products(
        &services
            .into_iter()
            .filter(|s| !ALWAYS.contains(s))
            .collect::<Vec<_>>(),
    )
    .map_err(|e| format!("{}: {e}", path.display()))
}

/// The products to install in `dir`: those its manifest lists, plus those `asked`. Products are only
/// ever added: removing one is not supported yet.
pub fn products_kept<S: AsRef<str>>(dir: &Path, asked: &[S]) -> Result<Vec<String>, String> {
    let mut products = installed_products(dir)?;
    products.extend(checked_products(asked)?);
    checked_products(&products)
}

/// Whether Lab was installed in `dir`: its client key is there.
pub fn lab_kept(dir: &Path) -> bool {
    dir.join("keys").join("lab-auth.pem").exists()
}

/// Write the instance's directory: secrets kept, every other file written again.
pub fn init(dir: &Path, instance: &Instance) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let secrets = secrets::Secrets::load_or_make(dir, instance)?;
    if instance.dev {
        dev_ca::write(dir, &instance.domain, &instance.hosted())?;
    }
    render::write_all(dir, instance, &secrets)
}
