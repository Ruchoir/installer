//! `ruchoir-install`: sets up a Ruchoir instance in a directory and runs it with Docker Compose.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::time::Duration;

use clap::{Parser, Subcommand};
use ruchoir_install::{garage, init, installed_products, products_kept, secrets, Instance};

#[derive(Parser)]
#[command(name = "ruchoir-install", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Write the instance's directory. Run again, it keeps the secrets and the keys and writes every
    /// other file anew.
    Init {
        /// The domain the services live under: Propolis at `id.<domain>`, the hub at `hub.<domain>`,
        /// the drive at `drive.<domain>`, the calendar at `calendar.<domain>`, the chat at
        /// `chat.<domain>`.
        #[arg(long)]
        domain: String,
        /// The address the certificate authority (Actalis) writes to about the certificates.
        #[arg(long)]
        acme_email: Option<String>,
        /// The products to install besides Propolis and the hub: `drive`, `calendar`, `chat`
        /// (which brings the drive). Those installed before are kept.
        #[arg(long, value_delimiter = ',')]
        products: Vec<String>,
        /// The instance's directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// Start the instance in the background, and prepare the drive's object store.
    Up {
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// Stop the instance; its data stays.
    Down {
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// A development instance on `*.localhost`, with a local certificate authority and a mail
    /// catcher, started.
    Dev {
        #[arg(long, default_value = "ruchoir-dev")]
        dir: PathBuf,
        /// The products to install besides Propolis and the hub: `drive`, `calendar`, `chat`
        /// (which brings the drive). Those installed before are kept.
        #[arg(long, value_delimiter = ',')]
        products: Vec<String>,
        /// A directory holding the repositories of the services (`propolis`, `hub`, and those of the
        /// products): their images are built from it first. Without it, the `ruchoir/<service>:dev`
        /// images must exist.
        #[arg(long)]
        source: Option<PathBuf>,
        /// Write the directory and build the images, without starting anything.
        #[arg(long)]
        no_start: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ruchoir-install: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Commands) -> Result<(), String> {
    match command {
        Commands::Init {
            domain,
            acme_email,
            products,
            dir,
        } => {
            let instance = Instance::production(&domain, acme_email)?
                .with_products(&products_kept(&dir, &products)?)?;
            init(&dir, &instance)?;
            // With the drive, the office editor's own host too.
            let hosts: Vec<String> = instance
                .hosted()
                .into_iter()
                .map(|s| instance.url(s))
                .collect();
            println!(
                "{} is ready: point {} at this machine, then run `ruchoir-install up --dir {}`.",
                dir.display(),
                hosts.join(", "),
                dir.display()
            );
            Ok(())
        }
        Commands::Up { dir } => {
            compose(&dir, &["up", "--detach", "--wait"])?;
            prepare(&dir)
        }
        Commands::Down { dir } => compose(&dir, &["down"]),
        Commands::Dev {
            dir,
            products,
            source,
            no_start,
        } => {
            let instance =
                Instance::development().with_products(&products_kept(&dir, &products)?)?;
            init(&dir, &instance)?;
            if let Some(source) = source {
                for service in instance.services() {
                    build(&source, service)?;
                }
            }
            if no_start {
                return Ok(());
            }
            compose(&dir, &["up", "--detach", "--wait"])?;
            prepare(&dir)?;
            println!(
                "Ruchoir runs at {}. Trust {} in your browser; mails are read at http://localhost:8025.",
                instance.url("propolis"),
                dir.join("dev-ca").join("ca.pem").display()
            );
            Ok(())
        }
    }
}

/// What a started instance needs before it is used: with the drive, Garage's layout, key and bucket.
fn prepare(dir: &Path) -> Result<(), String> {
    if installed_products(dir)?.iter().any(|p| p == "drive") {
        bootstrap_garage(dir)?;
    }
    Ok(())
}

/// `docker compose` on the instance's directory.
fn compose(dir: &Path, args: &[&str]) -> Result<(), String> {
    if !dir.join("compose.yaml").exists() {
        return Err(format!(
            "{} holds no instance: run `ruchoir-install init` first",
            dir.display()
        ));
    }
    // Run from the directory, so compose finds compose.yaml, compose.override.yaml and .env there.
    docker(
        Command::new("docker")
            .current_dir(dir)
            .arg("compose")
            .args(args),
    )
}

/// Garage's command line in its container, its output captured. The image has no shell.
fn garage_cli(dir: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("docker")
        .current_dir(dir)
        .args(["compose", "exec", "-T", "garage", "/garage"])
        .args(args)
        .output()
        .map_err(|e| format!("docker could not be run: {e}"))
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Prepare Garage for the drive (see `ruchoir_install::garage`): safe to run on every start.
fn bootstrap_garage(dir: &Path) -> Result<(), String> {
    let env = secrets::read_env(&dir.join(".env"));
    let get = |name: &str| {
        env.get(name)
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or_else(|| format!("{name} is missing from .env: run `ruchoir-install init` again"))
    };
    let (key_id, secret) = (
        get("DRIVE_S3_ACCESS_KEY_ID")?,
        get("DRIVE_S3_SECRET_ACCESS_KEY")?,
    );

    // Garage answers a few seconds after its container starts.
    let mut attempts = 0;
    while !garage_cli(dir, &["status"])?.status.success() {
        attempts += 1;
        if attempts >= 30 {
            return Err("Garage does not answer: see `docker compose logs garage`".to_owned());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let output = garage_cli(dir, &["node", "id", "-q"])?;
    let node = garage::node_id(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| format!("Garage's node id could not be read: {}", text(&output)))?;
    let output = garage_cli(dir, &["layout", "show"])?;
    let version = garage::layout_version(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| format!("Garage's layout could not be read: {}", text(&output)))?;

    for step in garage::bootstrap(&node, version, &key_id, &secret) {
        let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
        let output = garage_cli(dir, &args)?;
        let said = text(&output);
        let done_before = step.done_already.is_some_and(|s| said.contains(s));
        if !output.status.success() && !done_before {
            // What Garage says names the key at most, never its secret.
            return Err(format!("Garage could not {}: {}", step.what, said.trim()));
        }
    }
    println!("The drive's object store is ready.");
    Ok(())
}

/// Build a service's development image from its repository. Propolis's and the products' pages need
/// the organization's npm packages: the token is read from `NODE_AUTH_TOKEN`, as the build secret
/// `npm_token` their Dockerfiles name, never a layer.
fn build(source: &Path, service: &str) -> Result<(), String> {
    let repository = source.join(service);
    let mut command = Command::new("docker");
    command
        .arg("build")
        .arg("--tag")
        .arg(format!("ruchoir/{service}:dev"));
    if matches!(service, "propolis" | "drive" | "calendar" | "chat") {
        command
            .arg("--secret")
            .arg("id=npm_token,env=NODE_AUTH_TOKEN");
    }
    docker(command.arg(&repository))
}

fn docker(command: &mut Command) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|e| format!("docker could not be run: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("docker stopped: {status}"))
    }
}
