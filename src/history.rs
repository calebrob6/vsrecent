//! VS Code's recent-folder history: loading, parsing, classification, and filtering.

use crate::sqlite;
use serde_json::Value;
use std::path::PathBuf;

pub const LOADING_MESSAGE: &str = "Loading recent folders...";

#[cfg(windows)]
const HOME_VARIABLE: &str = "USERPROFILE";
#[cfg(not(windows))]
const HOME_VARIABLE: &str = "HOME";

#[derive(Clone, Debug)]
pub struct Entry {
    pub uri: String,
    pub label: String,
    pub remote: String,
    pub remote_key: String,
    pub search_key: String,
}

/// Reads the recent folders from VS Code's shared storage, which lives under the home directory
/// on every platform.
pub fn load_entries() -> Result<Vec<Entry>, String> {
    let home =
        std::env::var_os(HOME_VARIABLE).ok_or_else(|| format!("{HOME_VARIABLE} is not set"))?;
    let database = PathBuf::from(home)
        .join(".vscode-shared")
        .join("sharedStorage")
        .join("state.vscdb");
    if !database.is_file() {
        return Err(format!("database not found at {}", database.display()));
    }
    let json = sqlite::read_recent_json(&database)?
        .ok_or_else(|| "recent-folder history is missing".to_string())?;
    parse_entries(&json)
}

pub fn parse_entries(json: &str) -> Result<Vec<Entry>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|error| format!("invalid history JSON: {error}"))?;
    let items = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "history JSON does not contain an entries array".to_string())?;
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let Some(uri) = item.get("folderUri").and_then(Value::as_str) else {
            continue;
        };
        if uri.is_empty() {
            continue;
        }
        let label = item
            .get("label")
            .and_then(Value::as_str)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_label(uri));
        let remote = remote_kind(uri);
        let remote_key = remote_key(uri);
        let search_key = format!("{label} {uri} {remote}").to_lowercase();
        entries.push(Entry {
            uri: uri.to_string(),
            label,
            remote,
            remote_key,
            search_key,
        });
    }
    Ok(entries)
}

/// Returns the indices of the entries that belong to `remote_key` (any remote when `None`) and
/// contain every whitespace-separated token of `query`, ignoring case.
pub fn matching_entries(entries: &[Entry], query: &str, remote_key: Option<&str>) -> Vec<usize> {
    let query = query.to_lowercase();
    let tokens: Vec<&str> = query.split_whitespace().collect();
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            remote_key.is_none_or(|key| entry.remote_key == key)
                && tokens.iter().all(|token| entry.search_key.contains(token))
        })
        .map(|(index, _)| index)
        .collect()
}

/// The text displayed for an entry's row.
pub fn row_text(entry: &Entry) -> String {
    format!("{}    [{}]", entry.label, entry.remote)
}

/// The message displayed instead of rows when there is nothing to pick.
pub fn status_message(load_error: Option<&str>, total: usize, shown: usize) -> Option<String> {
    if let Some(error) = load_error {
        Some(error.to_string())
    } else if total == 0 {
        Some("No recent folders found".to_string())
    } else if shown == 0 {
        Some("No matching folders".to_string())
    } else {
        None
    }
}

/// The remote groups present in `entries` as `(remote key, menu title)`, ordered by key.
pub fn remote_groups(entries: &[Entry]) -> Vec<(String, String)> {
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for entry in entries {
        *counts.entry(entry.remote_key.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(key, count)| {
            (
                key.to_string(),
                format!("{} ({count})", remote_group_display(key)),
            )
        })
        .collect()
}

/// The menu title of the entry that clears the remote filter.
pub fn all_remotes_title(total: usize) -> String {
    format!("All remotes ({total})")
}

fn default_label(uri: &str) -> String {
    match uri.strip_prefix("file:///") {
        Some(path) => local_label(&percent_decode(path)),
        None => uri.to_string(),
    }
}

/// Formats the decoded path of a local `file:///` URI the way the platform writes paths.
#[cfg(windows)]
fn local_label(path: &str) -> String {
    path.replace('/', "\\")
}

/// Formats the decoded path of a local `file:///` URI the way the platform writes paths.
#[cfg(not(windows))]
fn local_label(path: &str) -> String {
    let home = std::env::var(HOME_VARIABLE).ok();
    abbreviate_home(&format!("/{path}"), home.as_deref())
}

/// Replaces a leading home directory with `~`, as VS Code's own Open Recent menu does.
#[cfg(not(windows))]
fn abbreviate_home(path: &str, home: Option<&str>) -> String {
    let home = home.map(|home| home.trim_end_matches('/')).unwrap_or("");
    if home.is_empty() {
        return path.to_string();
    }
    match path.strip_prefix(home) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

fn remote_kind(uri: &str) -> String {
    if uri
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    {
        return "LOCAL".to_string();
    }
    if let Some(rest) = strip_prefix_ascii_case(uri, "vscode-remote://") {
        let authority = rest.split('/').next().unwrap_or(rest);
        let decoded = percent_decode(authority);
        let mut parts = decoded.splitn(2, '+');
        let kind = parts.next().unwrap_or("");
        let instance = parts.next().unwrap_or("");
        return match kind.to_ascii_lowercase().as_str() {
            "wsl" => format!("WSL: {}", if instance.is_empty() { "?" } else { instance }),
            "ssh-remote" => {
                format!("SSH: {}", if instance.is_empty() { "?" } else { instance })
            }
            "dev-container" => "DEV CONTAINER".to_string(),
            "attached-container" => "CONTAINER".to_string(),
            "codespaces" => "CODESPACE".to_string(),
            "tunnel" => "TUNNEL".to_string(),
            _ => kind.to_ascii_uppercase(),
        };
    }
    if strip_prefix_ascii_case(uri, "vscode-vfs://github").is_some() {
        "GITHUB".to_string()
    } else {
        uri.split(':').next().unwrap_or("?").to_ascii_uppercase()
    }
}

fn remote_key(uri: &str) -> String {
    if uri
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    {
        return "local".to_string();
    }
    if let Some(rest) = strip_prefix_ascii_case(uri, "vscode-remote://") {
        let authority = rest.split('/').next().unwrap_or(rest);
        let decoded = percent_decode(authority);
        let kind = decoded.split('+').next().unwrap_or("").to_ascii_lowercase();
        return match kind.as_str() {
            "ssh-remote" => "ssh".to_string(),
            "attached-container" => "container".to_string(),
            "codespaces" => "codespace".to_string(),
            _ => kind,
        };
    }
    if strip_prefix_ascii_case(uri, "vscode-vfs://github").is_some() {
        "github".to_string()
    } else if strip_prefix_ascii_case(uri, "vscode-vfs://").is_some() {
        "vfs".to_string()
    } else {
        uri.split(':')
            .next()
            .filter(|scheme| !scheme.is_empty())
            .unwrap_or("unknown")
            .to_ascii_lowercase()
    }
}

fn remote_group_display(key: &str) -> String {
    match key {
        "local" => "Local".to_string(),
        "wsl" => "WSL".to_string(),
        "ssh" => "SSH".to_string(),
        "dev-container" => "Dev Container".to_string(),
        "container" => "Container".to_string(),
        "codespace" => "Codespace".to_string(),
        "tunnel" => "Tunnel".to_string(),
        "github" => "GitHub".to_string(),
        "vfs" => "VFS".to_string(),
        "unknown" => "Unknown".to_string(),
        _ => key.to_ascii_uppercase(),
    }
}

fn strip_prefix_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                decoded.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub fn demo_entries() -> Vec<Entry> {
    parse_entries(
        r#"{"entries":[
          {"label":"vsrecent-rust","folderUri":"file:///c%3A/Users/caleb/apps/vsrecent-rust"},
          {"label":"dotfiles","folderUri":"vscode-remote://wsl%2BUbuntu/home/caleb/dotfiles"},
          {"label":"training","folderUri":"vscode-remote://ssh-remote%2Bgpu-rig/data/training"},
          {"label":"api-service","folderUri":"vscode-remote://dev-container%2Babc/workspaces/api"}
        ]}"#,
    )
    .expect("demo JSON is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_folders_and_skips_files() {
        let entries = parse_entries(
            r#"{"entries":[
                {"folderUri":"file:///c%3A/code/project"},
                {"fileUri":"file:///c%3A/code/readme.md"},
                {"label":"remote","folderUri":"vscode-remote://wsl%2BUbuntu/home/me/app"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        #[cfg(windows)]
        assert_eq!(entries[0].label, "c:\\code\\project");
        #[cfg(not(windows))]
        assert_eq!(entries[0].label, "/c:/code/project");
        assert_eq!(entries[0].remote_key, "local");
        assert_eq!(entries[1].remote, "WSL: Ubuntu");
        assert_eq!(entries[1].remote_key, "wsl");
        assert!(entries[1].search_key.contains("wsl"));
    }

    #[test]
    fn decodes_utf8_uri_components() {
        assert_eq!(percent_decode("caf%C3%A9%20app"), "café app");
    }

    #[cfg(not(windows))]
    #[test]
    fn abbreviates_the_home_directory() {
        let home = Some("/Users/me");
        assert_eq!(abbreviate_home("/Users/me/code/app", home), "~/code/app");
        assert_eq!(abbreviate_home("/Users/me", home), "~");
        assert_eq!(abbreviate_home("/Users/me/", Some("/Users/me/")), "~/");
        assert_eq!(
            abbreviate_home("/Users/melody/app", home),
            "/Users/melody/app"
        );
        assert_eq!(abbreviate_home("/opt/app", home), "/opt/app");
        assert_eq!(abbreviate_home("/opt/app", None), "/opt/app");
        assert_eq!(abbreviate_home("/opt/app", Some("/")), "/opt/app");
    }

    #[test]
    fn filters_by_every_token_and_remote_group() {
        let entries = demo_entries();
        assert_eq!(matching_entries(&entries, "", None), [0, 1, 2, 3]);
        assert_eq!(matching_entries(&entries, "  TRAIN  gpu ", None), [2]);
        assert_eq!(matching_entries(&entries, "caleb", None), [0, 1]);
        assert_eq!(matching_entries(&entries, "caleb", Some("wsl")), [1]);
        assert_eq!(matching_entries(&entries, "ssh", None), [2]);
        assert!(matching_entries(&entries, "caleb missing", None).is_empty());
        assert!(matching_entries(&entries, "", Some("tunnel")).is_empty());
    }

    #[test]
    fn groups_remotes_by_key_with_counts() {
        let mut entries = demo_entries();
        entries.extend(demo_entries());
        assert_eq!(
            remote_groups(&entries),
            [
                ("dev-container".to_string(), "Dev Container (2)".to_string()),
                ("local".to_string(), "Local (2)".to_string()),
                ("ssh".to_string(), "SSH (2)".to_string()),
                ("wsl".to_string(), "WSL (2)".to_string()),
            ]
        );
        assert_eq!(all_remotes_title(entries.len()), "All remotes (8)");
    }

    #[test]
    fn explains_why_no_rows_are_shown() {
        assert_eq!(status_message(None, 4, 2), None);
        assert_eq!(
            status_message(None, 0, 0).as_deref(),
            Some("No recent folders found")
        );
        assert_eq!(
            status_message(None, 4, 0).as_deref(),
            Some("No matching folders")
        );
        assert_eq!(status_message(Some("boom"), 4, 2).as_deref(), Some("boom"));
        assert_eq!(row_text(&demo_entries()[1]), "dotfiles    [WSL: Ubuntu]");
    }
}
