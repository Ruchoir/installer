//! `ruchoir-install`: sets up a Ruchoir instance in a directory and runs it with Docker Compose.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};
use ruchoir_install::{init, Instance};

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
        /// The domain the services live under: Propolis at `id.<domain>`, the hub at `hub.<domain>`.
        #[arg(long)]
        domain: String,
        /// The address the certificate authority (Actalis) writes to about the certificates.
        #[arg(long)]
        acme_email: Option<String>,
        /// The products to install besides Propolis and the hub (none is available yet).
        #[arg(long, value_delimiter = ',')]
        products: Vec<String>,
        /// The instance's directory.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// Start the instance in the background.
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
        /// A directory holding the `propolis` and `hub` repositories: their images are built from it
        /// first. Without it, the `ruchoir/propolis:dev` and `ruchoir/hub:dev` images must exist.
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
            if let Some(product) = products.first() {
                return Err(format!("{product:?} cannot be installed yet"));
            }
            let instance = Instance::production(&domain, acme_email)?;
            init(&dir, &instance)?;
            println!(
                "{} is ready: point {} and {} at this machine, then run `ruchoir-install up --dir {}`.",
                dir.display(),
                instance.url("propolis"),
                instance.url("hub"),
                dir.display()
            );
            Ok(())
        }
        Commands::Up { dir } => compose(&dir, &["up", "--detach", "--wait"]),
        Commands::Down { dir } => compose(&dir, &["down"]),
        Commands::Dev {
            dir,
            source,
            no_start,
        } => {
            let instance = Instance::development();
            init(&dir, &instance)?;
            if let Some(source) = source {
                build(&source, "propolis")?;
                build(&source, "hub")?;
            }
            if no_start {
                return Ok(());
            }
            compose(&dir, &["up", "--detach", "--wait"])?;
            println!(
                "Ruchoir runs at {}. Trust {} in your browser; mails are read at http://localhost:8025.",
                instance.url("propolis"),
                dir.join("dev-ca").join("ca.pem").display()
            );
            Ok(())
        }
    }
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

/// Build a service's development image from its repository. Propolis's pages need the organization's
/// npm packages: the token is read from `NODE_AUTH_TOKEN`, as a build secret, never a layer.
fn build(source: &Path, service: &str) -> Result<(), String> {
    let repository = source.join(service);
    let mut command = Command::new("docker");
    command
        .arg("build")
        .arg("--tag")
        .arg(format!("ruchoir/{service}:dev"));
    if service == "propolis" {
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
