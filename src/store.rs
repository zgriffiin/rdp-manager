// Saved connections and settings, kept as JSON in %APPDATA%\RdpManager.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_PORT: u16 = 3389;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Audio {
    #[default]
    Local,
    Remote,
    None,
}

impl Audio {
    pub const ALL: [Audio; 3] = [Audio::Local, Audio::Remote, Audio::None];

    pub fn label(self) -> &'static str {
        match self {
            Audio::Local => "Play on this computer",
            Audio::Remote => "Play on remote computer",
            Audio::None => "Do not play",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub admin_session: bool,
    pub group: String,
    pub description: String,
    pub username: String,
    pub fullscreen: bool,
    /// Window size in pixels; `0 x 0` lets Remote Desktop choose.
    pub width: u32,
    pub height: u32,
    pub multi_monitor: bool,
    pub redirect_clipboard: bool,
    pub redirect_drives: bool,
    pub redirect_printers: bool,
    pub audio: Audio,
    pub created: u64,
    pub modified: u64,
    pub last_connected: Option<u64>,
}

impl Default for Connection {
    fn default() -> Self {
        let now = now();
        Self {
            id: new_id(),
            name: String::new(),
            host: String::new(),
            port: DEFAULT_PORT,
            admin_session: false,
            group: String::new(),
            description: String::new(),
            username: String::new(),
            fullscreen: false,
            width: 1600,
            height: 900,
            multi_monitor: false,
            redirect_clipboard: true,
            redirect_drives: false,
            redirect_printers: false,
            audio: Audio::Local,
            created: now,
            modified: now,
            last_connected: None,
        }
    }
}

impl Connection {
    pub fn address(&self) -> String {
        format_address(&self.host, self.port)
    }

    pub fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        [
            &self.name,
            &self.host,
            &self.group,
            &self.description,
            &self.username,
        ]
        .iter()
        .any(|f| f.to_lowercase().contains(&q))
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Data {
    pub theme: ThemeChoice,
    pub connections: Vec<Connection>,
}

pub struct Store {
    pub data: Data,
    path: PathBuf,
    /// Set when the existing file could not be read or moved aside; saving would overwrite it.
    read_only: bool,
}

impl Store {
    /// Loads the data file. A file that cannot be parsed is renamed to a backup
    /// instead of being overwritten, and the returned message explains this. If the
    /// file cannot be read or moved, the store is read-only so the file is never lost.
    pub fn load() -> (Self, Option<String>) {
        let dir = app_dir();
        let path = dir.join("connections.json");
        let mut warning = None;
        let mut read_only = false;
        let read_only_note =
            "Changes will not be saved until you fix or move the file and restart RDP Manager.";
        let data = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(data) => data,
                Err(err) => {
                    let backup = dir.join(format!("connections.broken-{}.json", now()));
                    warning = Some(match std::fs::rename(&path, &backup) {
                        Ok(()) => format!(
                            "Could not read saved connections ({err}). The file was moved to {}.",
                            backup.display()
                        ),
                        Err(e) => {
                            read_only = true;
                            format!(
                                "Could not read {} ({err}) or move it aside ({e}). {read_only_note}",
                                path.display()
                            )
                        }
                    });
                    Data::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => {
                read_only = true;
                warning = Some(format!(
                    "Could not read {} ({e}). {read_only_note}",
                    path.display()
                ));
                Data::default()
            }
        };
        (
            Self {
                data,
                path,
                read_only,
            },
            warning,
        )
    }

    pub fn save(&self) -> Result<(), String> {
        if self.read_only {
            return Err(format!(
                "Not saved: {} could not be read when RDP Manager started. Fix or move it, then restart.",
                self.path.display()
            ));
        }
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(app_dir())?;
            let json = serde_json::to_string_pretty(&self.data)?;
            // Write to a temporary file first so a failed write never corrupts the data.
            let tmp = self.path.with_extension("json.tmp");
            std::fs::write(&tmp, json)?;
            std::fs::rename(&tmp, &self.path)
        };
        write().map_err(|e| format!("Could not save connections: {e}"))
    }

    pub fn get(&self, id: &str) -> Option<&Connection> {
        self.data.connections.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Connection> {
        self.data.connections.iter_mut().find(|c| c.id == id)
    }

    pub fn upsert(&mut self, conn: Connection) {
        match self.get_mut(&conn.id) {
            Some(existing) => *existing = conn,
            None => self.data.connections.push(conn),
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.data.connections.retain(|c| c.id != id);
    }
}

pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("RdpManager")
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}{:x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn format_address(host: &str, port: u16) -> String {
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    if port == DEFAULT_PORT {
        host
    } else {
        format!("{host}:{port}")
    }
}

/// Splits `host`, `host:port`, `[ipv6]:port` or a bare IPv6 address.
/// The port is `None` when the input does not name one.
pub fn parse_address(input: &str) -> Option<(String, Option<u16>)> {
    let input = input.trim();
    if input.is_empty() || input.contains(char::is_whitespace) {
        return None;
    }
    let parse_port = |p: &str| p.parse::<u16>().ok().filter(|&p| p != 0);
    if let Some(rest) = input.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(parse_port(p)?),
            None if after.is_empty() => None,
            None => return None,
        };
        return (!host.is_empty()).then(|| (host.to_string(), port));
    }
    match input.matches(':').count() {
        0 => Some((input.to_string(), None)),
        1 => {
            let (host, port) = input.split_once(':')?;
            let port = parse_port(port)?;
            (!host.is_empty()).then(|| (host.to_string(), Some(port)))
        }
        _ => Some((input.to_string(), None)), // bare IPv6
    }
}

/// Short relative time such as "5 min ago".
pub fn ago(ts: u64) -> String {
    let d = now().saturating_sub(ts);
    match d {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", d / 60),
        3600..86400 => format!("{} h ago", d / 3600),
        86400..172800 => "yesterday".into(),
        172800..2592000 => format!("{} days ago", d / 86400),
        2592000..31536000 => format!("{} months ago", d / 2592000),
        _ => format!("{} years ago", d / 31536000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_addresses() {
        assert_eq!(
            parse_address(" 203.0.113.10 "),
            Some(("203.0.113.10".into(), None))
        );
        assert_eq!(
            parse_address("srv01:3390"),
            Some(("srv01".into(), Some(3390)))
        );
        assert_eq!(
            parse_address("[fe80::1]:4000"),
            Some(("fe80::1".into(), Some(4000)))
        );
        assert_eq!(parse_address("fe80::1"), Some(("fe80::1".into(), None)));
        assert_eq!(parse_address("srv01:0"), None);
        assert_eq!(parse_address("srv01:abc"), None);
        assert_eq!(parse_address("srv 01"), None);
        assert_eq!(parse_address(""), None);
    }

    #[test]
    fn formats_addresses() {
        assert_eq!(format_address("srv01", 3389), "srv01");
        assert_eq!(format_address("srv01", 3390), "srv01:3390");
        assert_eq!(format_address("fe80::1", 3390), "[fe80::1]:3390");
    }
}
