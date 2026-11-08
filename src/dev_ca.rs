//! The development certificate authority: in development, the proxy serves `*.localhost` with a
//! certificate this authority signed, and the services, which reach each other by their public
//! addresses (Propolis's design, decision of 2026-11-07), trust it through `SSL_CERT_FILE`.
//!
//! Made once and kept, so a browser told to trust it once keeps trusting it. Never used for a
//! production instance.

use std::path::Path;

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair};

use crate::secrets::write_private;

/// The names the development certificate covers.
pub fn names(domain: &str) -> Vec<String> {
    let mut names = vec![domain.to_owned(), format!("*.{domain}")];
    for service in crate::ALWAYS {
        let host = if service == "propolis" { "id" } else { service };
        names.push(format!("{host}.{domain}"));
    }
    names
}

/// Write `dev-ca/ca.pem` (the authority, public), `dev-ca/cert.pem` and `dev-ca/key.pem` (what the
/// proxy serves), unless they are there already.
pub fn write(dir: &Path, domain: &str) -> Result<(), String> {
    let ca_dir = dir.join("dev-ca");
    if ca_dir.join("ca.pem").exists() && ca_dir.join("cert.pem").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&ca_dir).map_err(|e| format!("{}: {e}", ca_dir.display()))?;
    let fail = |e: rcgen::Error| format!("development certificate: {e}");

    let ca_key = KeyPair::generate().map_err(fail)?;
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).map_err(fail)?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Ruchoir development authority");
    let ca_cert = ca_params.self_signed(&ca_key).map_err(fail)?;

    let leaf_key = KeyPair::generate().map_err(fail)?;
    let mut leaf_params = CertificateParams::new(names(domain)).map_err(fail)?;
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, format!("*.{domain}"));
    let issuer = Issuer::new(ca_params, ca_key);
    let leaf = leaf_params.signed_by(&leaf_key, &issuer).map_err(fail)?;

    std::fs::write(ca_dir.join("ca.pem"), ca_cert.pem())
        .map_err(|e| format!("{}: {e}", ca_dir.display()))?;
    std::fs::write(ca_dir.join("cert.pem"), leaf.pem())
        .map_err(|e| format!("{}: {e}", ca_dir.display()))?;
    write_private(&ca_dir.join("key.pem"), &leaf_key.serialize_pem())
}
