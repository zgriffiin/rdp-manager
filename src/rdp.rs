// Starts and tracks Remote Desktop sessions in the built-in Windows client (mstsc.exe).

use crate::credentials;
use crate::store::{self, Connection};
use std::collections::BTreeMap;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};

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
        if !conn.username.is_empty() {
            set_username_hint(&conn.address(), &conn.username);
        }
        match credentials::load_password(&conn.id) {
            Some(password) if !conn.username.is_empty() => {
                credentials::provide_to_remote_desktop(&conn.host, &conn.username, &password)?
            }
            // Never sign in with an entry left over from another connection to this host.
            _ => credentials::forget_remote_desktop(&conn.host),
        }
        let child = spawn(Command::new("mstsc.exe").args(mstsc_args(conn))).inspect_err(|_| {
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

/// Settings go on the command line instead of in an .rdp file: since the April 2026
/// Windows update, opening an .rdp file shows a security prompt on every launch.
/// Clipboard, drive, printer, and audio sharing follow Remote Desktop Connection's own defaults.
pub fn mstsc_args(c: &Connection) -> Vec<String> {
    let mut args = vec![format!("/v:{}", c.address())];
    if c.admin_session {
        args.push("/admin".into());
    }
    if c.fullscreen {
        args.push(if c.multi_monitor { "/multimon" } else { "/f" }.into());
    } else {
        args.push(format!("/w:{}", c.width));
        args.push(format!("/h:{}", c.height));
    }
    args
}

/// Fills in the user name when Remote Desktop asks for the password. Remote Desktop
/// Connection keeps this hint itself, so a failure only means the box starts empty.
fn set_username_hint(address: &str, user: &str) {
    let key = credentials::wide(&format!(
        r"Software\Microsoft\Terminal Server Client\Servers\{address}"
    ));
    let name = credentials::wide("UsernameHint");
    let value = credentials::wide(user);
    // SAFETY: all strings are null-terminated and outlive the call.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mstsc_args_reflect_settings() {
        let c = Connection {
            host: "srv01".into(),
            port: 3390,
            ..Default::default()
        };
        assert_eq!(mstsc_args(&c), ["/v:srv01:3390", "/w:1600", "/h:900"]);

        let full = Connection {
            fullscreen: true,
            admin_session: true,
            ..c
        };
        assert_eq!(mstsc_args(&full), ["/v:srv01:3390", "/admin", "/f"]);
        let all = Connection {
            multi_monitor: true,
            ..full
        };
        assert_eq!(mstsc_args(&all), ["/v:srv01:3390", "/admin", "/multimon"]);
    }
}
