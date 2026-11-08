//! Garage (Deuxfleurs, France; AGPL-3.0), the object store the drive keeps its files' bytes in: one
//! node inside the instance, reached by the drive at `http://garage:3900` on the instance's network
//! and published nowhere (phase 3 plan, decision 3).
//!
//! - [`config`]: `garage/garage.toml`. Its secrets are not in it: Garage reads `GARAGE_RPC_SECRET`
//!   and `GARAGE_ADMIN_TOKEN` from its environment, which compose fills from `.env`.
//! - [`bootstrap`]: a fresh Garage stores nothing until its layout is applied, the drive's key is
//!   imported and its bucket is created and allowed to the key. `ruchoir-install up` and `dev` run
//!   these commands after starting the instance, through `docker compose exec -T garage /garage`
//!   (the image has no shell). Each is safe to run again: the layout is applied only while none is,
//!   and "already exists" from the key or the bucket means the step was done before.
//!
//! The commands follow Garage v2's command line (`src/garage/cli/structs.rs` at `v2.5.0`): the flags
//! changed since v1.

/// Garage's image, pinned (Docker Hub, `dxflrs/garage`).
pub const IMAGE: &str = "dxflrs/garage:v2.5.0";
/// The bucket the drive keeps its files in.
pub const BUCKET: &str = "drive";
/// The name the drive's key is imported under.
pub const KEY_NAME: &str = "drive";
/// The zone of the single node.
pub const ZONE: &str = "dc1";
/// The capacity the node is given in the layout. Garage spreads data between nodes by capacity: with
/// one node, it only has to be set, and the disk is what bounds the files.
pub const CAPACITY: &str = "1T";

/// `garage/garage.toml`: a single node, metadata in SQLite, the S3 API on 3900, RPC on 3901 and the
/// admin API on 3903, all inside the instance's network. Based on the monolith's.
pub fn config() -> String {
    r#"# Garage, the drive's object store (Deuxfleurs, France), written by ruchoir-install: run
# `ruchoir-install init` again rather than editing it.
#
# One node, so one copy of each object. The secrets are not here: Garage reads GARAGE_RPC_SECRET
# and GARAGE_ADMIN_TOKEN from its environment, which compose.yaml fills from .env.

metadata_dir = "/var/lib/garage/meta"
data_dir = "/var/lib/garage/data"
db_engine = "sqlite"

replication_factor = 1

rpc_bind_addr = "[::]:3901"
rpc_public_addr = "127.0.0.1:3901"

[s3_api]
s3_region = "garage"
api_bind_addr = "[::]:3900"

[admin]
api_bind_addr = "[::]:3903"
"#
    .to_owned()
}

/// One command of the bootstrap: the arguments after `/garage`, what it does in words (the arguments
/// may hold a secret, the words never do), and the error text that means it was done already.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub args: Vec<String>,
    pub what: &'static str,
    pub done_already: Option<&'static str>,
}

fn step(what: &'static str, done_already: Option<&'static str>, args: &[&str]) -> Step {
    Step {
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        what,
        done_already,
    }
}

/// The commands that prepare Garage for the drive, for the node `node` whose applied layout is at
/// `layout_version` (`0`: none yet), with the drive's S3 key.
pub fn bootstrap(node: &str, layout_version: u64, key_id: &str, secret: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    if layout_version == 0 {
        steps.push(step(
            "give the node its role in the layout",
            None,
            &["layout", "assign", "-z", ZONE, "-c", CAPACITY, node],
        ));
        steps.push(step(
            "apply the layout",
            None,
            &["layout", "apply", "--version", "1"],
        ));
    }
    steps.push(step(
        "import the drive's key",
        Some("already exists"),
        &["key", "import", "--yes", "-n", KEY_NAME, key_id, secret],
    ));
    steps.push(step(
        "create the drive's bucket",
        Some("already exists"),
        &["bucket", "create", BUCKET],
    ));
    // By the key's id rather than its name: a name may be shared, an id never is.
    steps.push(step(
        "allow the drive's key on its bucket",
        None,
        &[
            "bucket", "allow", "--read", "--write", "--owner", BUCKET, "--key", key_id,
        ],
    ));
    steps
}

/// The node's id from what `garage node id -q` prints: `<64 hexadecimal characters>@<address>`.
pub fn node_id(output: &str) -> Option<String> {
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let id = line.split('@').next()?;
    (id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| id.to_owned())
}

/// The applied layout's version from what `garage layout show` prints
/// (`Current cluster layout version: <n>`).
pub fn layout_version(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Current cluster layout version:")
            .and_then(|v| v.trim().parse().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODE: &str = "563e1ac825ee3323aa45ae8fbb0e3cfd3d84f2c63e5e5b9d4cd5c07d73f7c9ab";

    #[test]
    fn a_fresh_garage_gets_a_layout_a_key_and_a_bucket() {
        let steps = bootstrap(NODE, 0, "GK0123456789abcdef01234567", "s3cr3t");
        let commands: Vec<String> = steps.iter().map(|s| s.args.join(" ")).collect();
        assert_eq!(
            commands,
            [
                format!("layout assign -z dc1 -c 1T {NODE}"),
                "layout apply --version 1".to_owned(),
                "key import --yes -n drive GK0123456789abcdef01234567 s3cr3t".to_owned(),
                "bucket create drive".to_owned(),
                "bucket allow --read --write --owner drive --key GK0123456789abcdef01234567"
                    .to_owned(),
            ]
        );
        // What a step says of itself never holds the secret.
        assert!(steps.iter().all(|s| !s.what.contains("s3cr3t")));
    }

    #[test]
    fn a_garage_with_a_layout_keeps_it_and_tolerates_what_exists() {
        let steps = bootstrap(NODE, 3, "GK0123456789abcdef01234567", "s3cr3t");
        assert!(steps.iter().all(|s| s.args[0] != "layout"));
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].done_already, Some("already exists"));
        assert_eq!(steps[1].done_already, Some("already exists"));
    }

    #[test]
    fn the_node_and_the_layout_are_read_from_garages_output() {
        assert_eq!(
            node_id(&format!("{NODE}@127.0.0.1:3901\n")).as_deref(),
            Some(NODE)
        );
        assert_eq!(node_id(&format!("\n{NODE}\n")).as_deref(), Some(NODE));
        assert_eq!(node_id("Error: unable to read node key"), None);

        let show = "==== CURRENT CLUSTER LAYOUT ====\nNo nodes currently have a role in the cluster.\n\nCurrent cluster layout version: 0\n";
        assert_eq!(layout_version(show), Some(0));
        assert_eq!(
            layout_version("Current cluster layout version: 12\n==== STAGED ROLE CHANGES ===="),
            Some(12)
        );
        assert_eq!(layout_version("nothing"), None);
    }

    #[test]
    fn the_configuration_holds_no_secret() {
        let config = config();
        let table: toml::Table = config.parse().unwrap();
        assert_eq!(table["replication_factor"].as_integer(), Some(1));
        assert_eq!(table["db_engine"].as_str(), Some("sqlite"));
        assert_eq!(table["s3_api"]["api_bind_addr"].as_str(), Some("[::]:3900"));
        assert_eq!(table["admin"]["api_bind_addr"].as_str(), Some("[::]:3903"));
        assert!(!config.contains("rpc_secret") && !config.contains("admin_token"));
    }
}
