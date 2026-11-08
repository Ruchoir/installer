//! The development certificate authority: in development, the proxy serves `*.localhost` with a
//! certificate this authority signed, and the services, which reach each other by their public
//! addresses (Propolis's design, decision of 2026-11-07), trust it through `SSL_CERT_FILE`.
//!
//! The authority is made once and kept, so a browser told to trust it once keeps trusting it. The
//! certificate the proxy serves names every service's host; when a product is added, it is issued
//! again by the same authority (`dev-ca/ca-key.pem`). Never used for a production instance.

use std::path::Path;

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair};

use crate::secrets::write_private;

/// The names the development certificate covers: the domain, its wildcard, and each service's host.
pub fn names(domain: &str, services: &[&str]) -> Vec<String> {
    let mut names = vec![domain.to_owned(), format!("*.{domain}")];
    for service in services {
        let host = if *service == "propolis" {
            "id"
        } else {
            service
        };
        names.push(format!("{host}.{domain}"));
    }
    names
}

fn fail(e: rcgen::Error) -> String {
    format!("development certificate: {e}")
}

/// The authority's parameters, the same each time, so a kept key signs as the kept certificate.
fn authority_params() -> Result<CertificateParams, String> {
    let mut params = CertificateParams::new(Vec::<String>::new()).map_err(fail)?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, "Ruchoir development authority");
    Ok(params)
}

/// Write `dev-ca/ca.pem` (the authority, public) and its key `dev-ca/ca-key.pem`, then
/// `dev-ca/cert.pem` and `dev-ca/key.pem` (what the proxy serves) with the names they cover in
/// `dev-ca/names`. What is there already is kept, unless the certificate lacks a service's host.
pub fn write(dir: &Path, domain: &str, services: &[&str]) -> Result<(), String> {
    let ca_dir = dir.join("dev-ca");
    let path = |name: &str| ca_dir.join(name);
    let read = |name: &str| std::fs::read_to_string(path(name)).ok();
    let save = |name: &str, text: &str| {
        std::fs::write(path(name), text).map_err(|e| format!("{}: {e}", path(name).display()))
    };
    let names = names(domain, services);
    let listed = names.join("\n") + "\n";
    let has_cert = path("cert.pem").exists();
    if has_cert && read("names").as_deref() == Some(listed.as_str()) {
        return Ok(());
    }
    std::fs::create_dir_all(&ca_dir).map_err(|e| format!("{}: {e}", ca_dir.display()))?;

    let issuer = match (read("ca.pem"), read("ca-key.pem")) {
        (Some(_), Some(key)) => {
            Issuer::new(authority_params()?, KeyPair::from_pem(&key).map_err(fail)?)
        }
        // An authority made before its key was kept: its certificate stays, covering the new hosts
        // through `*.<domain>`, since nothing could sign a new one.
        (Some(_), None) if has_cert => return Ok(()),
        _ => {
            let key = KeyPair::generate().map_err(fail)?;
            let params = authority_params()?;
            let cert = params.self_signed(&key).map_err(fail)?;
            save("ca.pem", &cert.pem())?;
            write_private(&path("ca-key.pem"), &key.serialize_pem())?;
            Issuer::new(params, key)
        }
    };

    let leaf_key = KeyPair::generate().map_err(fail)?;
    let mut leaf_params = CertificateParams::new(names).map_err(fail)?;
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, format!("*.{domain}"));
    let leaf = leaf_params.signed_by(&leaf_key, &issuer).map_err(fail)?;
    save("cert.pem", &leaf.pem())?;
    write_private(&path("key.pem"), &leaf_key.serialize_pem())?;
    save("names", &listed)
}
