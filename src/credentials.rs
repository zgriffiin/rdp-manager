// Passwords live in Windows Credential Manager (encrypted for the current Windows user),
// never in the JSON data file.

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST, CRED_PERSIST_LOCAL_MACHINE, CRED_PERSIST_SESSION, CRED_TYPE_GENERIC, CREDENTIALW,
    CredDeleteW, CredFree, CredReadW, CredWriteW,
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn app_target(id: &str) -> String {
    format!("RdpManager/{id}")
}

/// Marks the `TERMSRV/<host>` entries this app writes, so it only ever removes its own.
const MARKER: &str = "Added by RDP Manager";

fn write(
    target: &str,
    user: &str,
    password: &str,
    persist: CRED_PERSIST,
    comment: Option<&str>,
) -> Result<(), String> {
    let mut target_w = wide(target);
    let mut user_w = wide(user);
    let mut comment_w = comment.map(wide);
    let mut blob: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let cred = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target_w.as_mut_ptr(),
        Comment: comment_w.as_mut().map_or(null_mut(), |c| c.as_mut_ptr()),
        LastWritten: FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: persist,
        AttributeCount: 0,
        Attributes: null_mut(),
        TargetAlias: null_mut(),
        UserName: user_w.as_mut_ptr(),
    };
    // SAFETY: every pointer in `cred` refers to a live buffer owned by this function.
    let ok = unsafe { CredWriteW(&cred, 0) } != 0;
    blob.fill(0);
    if ok {
        Ok(())
    } else {
        Err(format!(
            "Windows Credential Manager refused to save the password: {}",
            std::io::Error::last_os_error()
        ))
    }
}

struct Stored {
    password: String,
    comment: Option<String>,
}

fn read(target: &str) -> Option<Stored> {
    let target = wide(target);
    let mut cred: *mut CREDENTIALW = null_mut();
    // SAFETY: `target` is NUL-terminated; on success `cred` points to a valid CREDENTIALW
    // until it is freed with CredFree below. Null blob/comment pointers are checked.
    unsafe {
        if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) == 0 {
            return None;
        }
        let c = &*cred;
        let bytes = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
        };
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        let password = String::from_utf16_lossy(&units);
        let comment = (!c.Comment.is_null()).then(|| {
            let len = (0..).take_while(|&i| *c.Comment.add(i) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(c.Comment, len))
        });
        CredFree(cred as *const _);
        Some(Stored { password, comment })
    }
}

fn delete(target: &str) {
    let target = wide(target);
    // SAFETY: `target` is NUL-terminated. A missing credential is not an error here.
    unsafe {
        CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0);
    }
}

/// Saves the password for a saved connection.
pub fn save_password(id: &str, user: &str, password: &str) -> Result<(), String> {
    write(
        &app_target(id),
        user,
        password,
        CRED_PERSIST_LOCAL_MACHINE,
        None,
    )
}

/// Returns the saved password for a connection, if any.
pub fn load_password(id: &str) -> Option<String> {
    read(&app_target(id)).map(|c| c.password)
}

pub fn delete_password(id: &str) {
    delete(&app_target(id));
}

fn termsrv(host: &str) -> String {
    format!("TERMSRV/{host}")
}

/// Hands the credentials to the Remote Desktop client (`TERMSRV/<host>`, the same entry
/// `cmdkey /generic:TERMSRV/<host>` creates). Kept only until you sign out of Windows,
/// and removed by [`forget_remote_desktop`] when the session ends. An entry the user
/// saved themselves is left alone and wins.
pub fn provide_to_remote_desktop(host: &str, user: &str, password: &str) -> Result<(), String> {
    let target = termsrv(host);
    if read(&target).is_some_and(|c| c.comment.as_deref() != Some(MARKER)) {
        return Ok(());
    }
    write(&target, user, password, CRED_PERSIST_SESSION, Some(MARKER))
}

/// Removes the `TERMSRV/<host>` entry if this app wrote it.
pub fn forget_remote_desktop(host: &str) {
    let target = termsrv(host);
    if read(&target).is_some_and(|c| c.comment.as_deref() == Some(MARKER)) {
        delete(&target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trip() {
        let id = format!("test-{}", std::process::id());
        save_password(&id, "user", "pä$$wörd").unwrap();
        assert_eq!(load_password(&id).as_deref(), Some("pä$$wörd"));
        delete_password(&id);
        assert_eq!(load_password(&id), None);
    }

    #[test]
    fn remote_desktop_entry_is_removed_only_when_ours() {
        let host = format!("rdp-manager-test-{}.invalid", std::process::id());
        provide_to_remote_desktop(&host, "user", "pw").unwrap();
        assert!(read(&termsrv(&host)).is_some());
        forget_remote_desktop(&host);
        assert!(read(&termsrv(&host)).is_none());

        // An entry the user saved (no marker) is neither replaced nor removed.
        write(
            &termsrv(&host),
            "theirs",
            "keep",
            CRED_PERSIST_SESSION,
            None,
        )
        .unwrap();
        provide_to_remote_desktop(&host, "user", "pw").unwrap();
        forget_remote_desktop(&host);
        assert_eq!(
            read(&termsrv(&host)).map(|c| c.password).as_deref(),
            Some("keep")
        );
        delete(&termsrv(&host));
    }
}
