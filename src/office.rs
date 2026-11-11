//! Euro-Office, the office engine, installed with the drive (always with the drive, never on its
//! own). The drive is its WOPI host: the engine opens and saves documents through the drive's WOPI
//! listener (`http://drive:8081`), and browsers reach the editor at a hostname of its own,
//! `office.<domain>`, which the proxy routes to the drive and the drive relays to the engine. The
//! engine is published nowhere and has no route out: it sits on a network of its own, `office`,
//! that only the drive shares.
//!
//! - [`FILES`]: the files the engine mounts read-only, its settings, the Ruchoir themes and two
//!   patched pages. They are copied, unchanged, from the monolith's `infra/office/` and built into
//!   the installer, which writes them into the instance's `office/` directory. None of them names a
//!   host or anything else of an instance.
//! - [`service`]: the engine's service in `compose.yaml`.
//! - [`drive_env`]: what the drive is told to reach the engine and to be reached by it.
//!
//! The patched pages were taken from Euro-Office 9.3.4: when [`IMAGE`] moves, each patch is checked
//! against the new version and taken again from its image (see `AGENTS.md`).

/// Euro-Office's image, pinned: the monolith's (GitHub's registry, `euro-office/documentserver`).
pub const IMAGE: &str = "ghcr.io/euro-office/documentserver:v9.3.4-hotfix.1";
/// The engine's service name, and its host on the `office` network.
pub const SERVICE: &str = "office";
/// The memory the engine may take, unless `OFFICE_MEM_LIMIT` in `.env` says otherwise. It holds
/// about 2 GB at rest, measured on the monolith.
pub const MEM_LIMIT: &str = "4g";

/// The engine's web pages, as a literal for `concat!`.
macro_rules! web_apps {
    () => {
        "/var/www/euro-office/documentserver/web-apps/apps"
    };
}

/// Where the engine reads its settings.
const SETTINGS: &str = "/etc/euro-office/documentserver/local-production-linux.json";

/// One file the engine mounts read-only.
#[derive(Debug, Clone, Copy)]
pub struct File {
    /// Where it is written, under the instance's directory.
    pub path: &'static str,
    /// Where the engine reads it.
    pub target: &'static str,
    /// Its bytes, as copied from the monolith.
    pub bytes: &'static [u8],
}

/// A file of `assets/office/`, written to `office/` and mounted at `target`.
macro_rules! file {
    ($path:expr, $target:expr) => {
        File {
            path: concat!("office/", $path),
            target: $target,
            bytes: include_bytes!(concat!("../assets/office/", $path)),
        }
    };
}

/// A theme, mounted under its own name: the drive's sessions ask for it by that name.
macro_rules! theme {
    ($name:literal) => {
        file!(
            concat!("themes/", $name, ".json"),
            concat!(
                web_apps!(),
                "/common/main/resources/themes/",
                $name,
                ".json"
            )
        )
    };
}

/// The files the engine mounts: its settings (a save every ten minutes in a long session), the
/// themes the drive's sessions ask for (`theme-ruchoir-light`, `theme-ruchoir-<style>-light`), and
/// the patched WOPI page and Visio viewer page.
pub const FILES: [File; 9] = [
    file!("local-production-linux.json", SETTINGS),
    theme!("theme-ruchoir-light"),
    theme!("theme-ruchoir-dark"),
    theme!("theme-ruchoir-aubergine-light"),
    theme!("theme-ruchoir-nuage-light"),
    theme!("theme-ruchoir-indigo-light"),
    theme!("theme-ruchoir-marine-light"),
    file!(
        "patches/editor-wopi.ejs",
        concat!(web_apps!(), "/api/wopi/editor-wopi.ejs")
    ),
    file!(
        "patches/visioeditor-index.html",
        concat!(web_apps!(), "/visioeditor/main/index.html")
    ),
];

/// The editor's public address, `https://office.<domain>`: a hostname of its own, never the drive's,
/// routed by the proxy to the drive.
pub fn public_url(domain: &str) -> String {
    format!("https://{SERVICE}.{domain}")
}

/// The drive's settings for the engine (`DRIVE_OFFICE_*`, `DRIVE_WOPI_*` of the drive's
/// `src/config.rs`), as compose environment lines. The WOPI listener's address and base are the
/// drive's own defaults in its image, written out so the instance says how its parts meet.
pub fn drive_env(domain: &str) -> String {
    format!(
        "      DRIVE_OFFICE_URL: http://{SERVICE}\n\
         \x20     DRIVE_OFFICE_PUBLIC_URL: {}\n\
         \x20     DRIVE_WOPI_LISTEN: 0.0.0.0:8081\n\
         \x20     DRIVE_WOPI_BASE_URL: http://drive:8081\n",
        public_url(domain)
    )
}

/// The engine's service in `compose.yaml`, as the monolith ran it.
pub fn service() -> String {
    let volumes: String = FILES
        .iter()
        .map(|f| format!("      - ./{}:{}:ro\n", f.path, f.target))
        .collect();
    format!(
        r#"
  # The office engine (Euro-Office, AGPL-3.0), the drive's live editor. It calls the drive's WOPI
  # listener, and browsers reach it through the drive at office.<domain>. About 2 GB of memory at
  # rest and 7 GB of disk for the image.
  {SERVICE}:
    image: {IMAGE}
    environment:
      # The engine refuses its own (non-WOPI) API without a signed token. The drive never calls that
      # API, so the secret stays empty and the engine draws a random one at start.
      JWT_ENABLED: "true"
      JWT_SECRET: ""
      WOPI_ENABLED: "true"
      # The drive's WOPI listener is on a private network.
      ALLOW_PRIVATE_IP_ADDRESS: "true"
      EXAMPLE_ENABLED: "false"
    volumes:
{volumes}    mem_limit: ${{OFFICE_MEM_LIMIT:-{MEM_LIMIT}}}
    # The engine bundles several services and writes widely on its own filesystem: no capability
    # dropped and no read-only root, which could break it in ways only a long session shows.
    security_opt: [no-new-privileges:true]
    # Its own network alone, with no route out: whatever a document asks it to fetch, it reaches
    # nothing but the drive.
    networks: [{SERVICE}]
    # Its first start writes its fonts and themes, which took six and a half minutes on a busy
    # development machine: failures count only after ten minutes, and the first answer at any time
    # makes it healthy, so `up --wait` waits for a slow first start rather than failing it.
    healthcheck:
      test: ["CMD-SHELL", "curl -fsS http://127.0.0.1/healthcheck || exit 1"]
      interval: 30s
      timeout: 5s
      retries: 5
      start_period: 600s
    restart: unless-stopped
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the engine gathers its interface themes, one per file.
    const THEMES: &str = concat!(web_apps!(), "/common/main/resources/themes");
    /// The engine's web pages.
    const WEB_APPS: &str = web_apps!();

    #[test]
    fn every_file_is_under_office_and_mounted_where_the_engine_reads_it() {
        for file in FILES {
            assert!(file.path.starts_with("office/"), "{}", file.path);
            assert!(!file.bytes.is_empty(), "{}", file.path);
            assert!(
                file.target == SETTINGS
                    || file.target.starts_with(&format!("{THEMES}/"))
                    || file.target.starts_with(&format!("{WEB_APPS}/")),
                "{}",
                file.target
            );
        }
        // A theme is mounted under its own name: the drive asks for it by that name.
        for file in FILES
            .iter()
            .filter(|f| f.path.starts_with("office/themes/"))
        {
            let name = file.path.rsplit('/').next().unwrap();
            assert_eq!(file.target, format!("{THEMES}/{name}"));
        }
    }

    #[test]
    fn the_settings_and_themes_are_json_and_name_no_host() {
        for file in FILES {
            let text = std::str::from_utf8(file.bytes).unwrap();
            if file.path.ends_with(".json") {
                serde_json::from_str::<serde_json::Value>(text).unwrap();
            }
            for host in ["ruchoir.com", "ruchoir.fr", ".localhost", "example.org"] {
                assert!(!text.contains(host), "{} names {host}", file.path);
            }
        }
    }

    #[test]
    fn the_drive_reaches_the_engine_and_is_reached_by_it() {
        assert_eq!(public_url("example.org"), "https://office.example.org");
        let env = drive_env("example.org");
        let lines: Vec<&str> = env.lines().map(str::trim).collect();
        assert_eq!(
            lines,
            [
                "DRIVE_OFFICE_URL: http://office",
                "DRIVE_OFFICE_PUBLIC_URL: https://office.example.org",
                "DRIVE_WOPI_LISTEN: 0.0.0.0:8081",
                "DRIVE_WOPI_BASE_URL: http://drive:8081",
            ]
        );
        assert!(env.lines().all(|l| l.starts_with("      DRIVE_")));
    }

    #[test]
    fn the_service_mounts_every_file_read_only() {
        let service = service();
        for file in FILES {
            assert!(service.contains(&format!("- ./{}:{}:ro\n", file.path, file.target)));
        }
        assert!(service.contains(&format!("image: {IMAGE}")));
        assert!(service.contains("mem_limit: ${OFFICE_MEM_LIMIT:-4g}"));
        assert!(service.contains("networks: [office]"));
    }
}
