//! Host entries for a development instance, for the programs that do not resolve `*.localhost`
//! themselves.
//!
//! Browsers send every `*.localhost` name to the loopback (RFC 6761), and so does the resolver of
//! most Linux systems. Windows does not: outside a browser (Python, Node, `curl`, the desktop app)
//! `id.localhost` is not found. `ruchoir-install hosts` prints the lines that fix it, one per host of
//! the instance, and with `--write` puts them in the system's hosts file between two marker lines,
//! replacing what an earlier run wrote there and nothing else.

use std::path::PathBuf;

/// The line opening the block this module owns in a hosts file.
pub const BEGIN: &str = "# ruchoir-dev: begin (written by ruchoir-install hosts)";
/// The line closing it.
pub const END: &str = "# ruchoir-dev: end";

/// The block for these host names: each on IPv4 and IPv6 loopback, between the markers.
pub fn block(names: &[String]) -> String {
    let mut out = format!("{BEGIN}\n");
    for name in names {
        out.push_str(&format!("127.0.0.1 {name}\n::1 {name}\n"));
    }
    out.push_str(END);
    out.push('\n');
    out
}

/// `existing` with its Ruchoir block replaced by `block`, or `block` appended when it had none.
/// Everything outside the markers is kept as it was, line endings included.
pub fn merge(existing: &str, block: &str) -> String {
    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let block = block.replace('\n', newline);
    if let (Some(start), Some(end_at)) = (existing.find(BEGIN), existing.find(END)) {
        if end_at > start {
            let mut after = end_at + END.len();
            if existing[after..].starts_with(newline) {
                after += newline.len();
            }
            return format!("{}{}{}", &existing[..start], block, &existing[after..]);
        }
    }
    let mut out = existing.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(newline);
    }
    out.push_str(&block);
    out
}

/// The system's hosts file.
pub fn system_file() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root)
            .join("System32")
            .join("drivers")
            .join("etc")
            .join("hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        vec!["id.localhost".to_owned(), "hub.localhost".to_owned()]
    }

    #[test]
    fn the_block_sends_each_host_to_both_loopbacks() {
        let b = block(&names());
        assert!(b.starts_with(BEGIN));
        assert!(b.contains("127.0.0.1 id.localhost\n::1 id.localhost\n"));
        assert!(b.contains("127.0.0.1 hub.localhost\n"));
        assert!(b.ends_with(&format!("{END}\n")));
    }

    #[test]
    fn a_file_without_a_block_gets_it_appended() {
        let merged = merge("127.0.0.1 localhost", &block(&names()));
        assert!(merged.starts_with("127.0.0.1 localhost\n# ruchoir-dev: begin"));
    }

    #[test]
    fn an_earlier_block_is_replaced_and_the_rest_kept() {
        let first = merge("# mine\r\n10.0.0.1 nas\r\n", &block(&names()));
        assert!(first.contains("127.0.0.1 id.localhost\r\n"));
        let second = merge(
            &format!("{first}192.168.0.2 printer\r\n"),
            &block(&["drive.localhost".to_owned()]),
        );
        assert!(second.starts_with("# mine\r\n10.0.0.1 nas\r\n"));
        assert!(!second.contains("id.localhost"));
        assert!(second.contains("127.0.0.1 drive.localhost\r\n"));
        assert!(second.ends_with("# ruchoir-dev: end\r\n192.168.0.2 printer\r\n"));
        assert_eq!(second.matches(BEGIN).count(), 1);
    }

    #[test]
    fn merging_twice_changes_nothing() {
        let once = merge("127.0.0.1 localhost\n", &block(&names()));
        assert_eq!(merge(&once, &block(&names())), once);
    }
}
