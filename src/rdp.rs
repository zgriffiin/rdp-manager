// Starts and tracks Remote Desktop sessions in the built-in Windows client (mstsc.exe).

use crate::credentials;
use crate::store::{self, Audio, Connection};
use std::collections::BTreeMap;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// How long mstsc may still read the shared `TERMSRV/<host>` sign-in entry after it starts.
const SIGN_IN_TIME: Duration = Duration::from_secs(30);

pub struct Session {
    pub label: String,
    pub address: String,
    pub started: Instant,
    /// Host name, used to remove the sign-in credential when the session ends.
    host: String,
    child: Child,
}

#[derive(Default)]
pub struct Sessions {
    map: BTreeMap<String, Session>,
}

impl Sessions {
    /// Forgets sessions whose Remote Desktop window was closed.
    pub fn poll(&mut self) {
        let mut ended = Vec::new();
        self.map.retain(|_, s| {
            let running = matches!(s.child.try_wait(), Ok(None));
            if !running {
                ended.push(std::mem::take(&mut s.host));
            }
            running
        });
        for host in ended {
            self.forget_if_unused(&host);
        }
    }

    /// `TERMSRV/<host>` is one entry per host, so it stays while any session to that host is open.
    fn forget_if_unused(&self, host: &str) {
        if !self.map.values().any(|s| s.host.eq_ignore_ascii_case(host)) {
            credentials::forget_remote_desktop(host);
        }
    }

    /// Refuses to start while another session to the same host may still be reading the
    /// shared sign-in entry, so neither session can sign in with the other's account.
    fn check_host_free(&self, host: &str) -> Result<(), String> {
        let signing_in = self
            .map
            .values()
            .any(|s| s.host.eq_ignore_ascii_case(host) && s.started.elapsed() < SIGN_IN_TIME);
        if signing_in {
            Err(format!(
                "Another session to {host} is still signing in. Try again in a few seconds."
            ))
        } else {
            Ok(())
        }
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Session)> {
        self.map.iter()
    }

    pub fn connect(&mut self, conn: &Connection) -> Result<(), String> {
        self.check_host_free(&conn.host)?;
        let dir = store::app_dir().join("sessions");
        let path = dir.join(format!("{}.rdp", conn.id));
        std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&path, encode_utf16_bom(&rdp_file(conn))))
            .map_err(|e| format!("Could not write the connection file: {e}"))?;
        match credentials::load_password(&conn.id) {
            Some(password) if !conn.username.is_empty() => {
                credentials::provide_to_remote_desktop(&conn.host, &conn.username, &password)?
            }
            // Never sign in with an entry left over from another connection to this host.
            _ => credentials::forget_remote_desktop(&conn.host),
        }
        let child = spawn(Command::new("mstsc.exe").arg(&path)).inspect_err(|_| {
            credentials::forget_remote_desktop(&conn.host);
        })?;
        self.insert(
            conn.id.clone(),
            conn.name.clone(),
            conn.address(),
            conn.host.clone(),
            child,
        );
        Ok(())
    }

    /// Connects to an address that is not saved. Returns the session id.
    pub fn quick_connect(&mut self, host: &str, port: u16) -> Result<String, String> {
        let address = store::format_address(host, port);
        let id = format!("quick:{address}");
        if !self.is_active(&id) {
            self.check_host_free(host)?;
            credentials::forget_remote_desktop(host);
            let child = spawn(Command::new("mstsc.exe").arg(format!("/v:{address}")))?;
            self.insert(
                id.clone(),
                address.clone(),
                address,
                host.to_string(),
                child,
            );
        }
        Ok(id)
    }

    /// Closes the Remote Desktop window. The remote session stays signed in, just
    /// like closing the window yourself.
    pub fn disconnect(&mut self, id: &str) {
        if let Some(mut s) = self.map.remove(id) {
            let _ = s.child.kill();
            let _ = s.child.wait();
            self.forget_if_unused(&s.host);
        }
    }

    fn insert(&mut self, id: String, label: String, address: String, host: String, child: Child) {
        self.map.insert(
            id,
            Session {
                label,
                address,
                started: Instant::now(),
                host,
                child,
            },
        );
    }
}

impl Drop for Sessions {
    /// Open Remote Desktop windows keep running after the app closes; they are already
    /// signed in, so their credentials are no longer needed.
    fn drop(&mut self) {
        for s in self.map.values() {
            credentials::forget_remote_desktop(&s.host);
        }
    }
}

fn spawn(cmd: &mut Command) -> Result<Child, String> {
    cmd.spawn()
        .map_err(|e| format!("Could not start Remote Desktop (mstsc.exe): {e}"))
}

fn encode_utf16_bom(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    out
}

pub fn rdp_file(c: &Connection) -> String {
    let flag = |b: bool| if b { 1 } else { 0 };
    let mut lines = vec![
        format!("full address:s:{}", c.address()),
        format!("screen mode id:i:{}", if c.fullscreen { 2 } else { 1 }),
        format!("use multimon:i:{}", flag(c.fullscreen && c.multi_monitor)),
        "session bpp:i:32".into(),
        "smart sizing:i:1".into(),
        "dynamic resolution:i:1".into(),
        "autoreconnection enabled:i:1".into(),
        "authentication level:i:2".into(),
        "prompt for credentials:i:0".into(),
        format!("administrative session:i:{}", flag(c.admin_session)),
        format!("redirectclipboard:i:{}", flag(c.redirect_clipboard)),
        format!("redirectprinters:i:{}", flag(c.redirect_printers)),
        format!(
            "drivestoredirect:s:{}",
            if c.redirect_drives { "*" } else { "" }
        ),
        format!(
            "audiomode:i:{}",
            match c.audio {
                Audio::Local => 0,
                Audio::Remote => 1,
                Audio::None => 2,
            }
        ),
    ];
    if !c.fullscreen && c.width > 0 && c.height > 0 {
        lines.push(format!("desktopwidth:i:{}", c.width));
        lines.push(format!("desktopheight:i:{}", c.height));
    }
    if !c.username.is_empty() {
        lines.push(format!("username:s:{}", c.username));
    }
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdp_file_reflects_settings() {
        let c = Connection {
            host: "srv01".into(),
            port: 3390,
            username: r"CORP\me".into(),
            redirect_drives: true,
            audio: Audio::None,
            ..Default::default()
        };
        let text = rdp_file(&c);
        for line in [
            "full address:s:srv01:3390",
            "screen mode id:i:1",
            "drivestoredirect:s:*",
            "audiomode:i:2",
            "desktopwidth:i:1600",
            r"username:s:CORP\me",
        ] {
            assert!(
                text.contains(&format!("{line}\r\n")),
                "missing {line:?} in:\n{text}"
            );
        }

        let full = rdp_file(&Connection {
            fullscreen: true,
            multi_monitor: true,
            ..c
        });
        assert!(full.contains("screen mode id:i:2\r\n") && full.contains("use multimon:i:1\r\n"));
        assert!(!full.contains("desktopwidth"));
    }

    #[test]
    fn rdp_file_is_utf16_with_bom() {
        assert_eq!(encode_utf16_bom("a"), [0xFF, 0xFE, b'a', 0]);
    }
}
