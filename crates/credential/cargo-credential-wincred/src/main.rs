//! Cargo registry windows credential process.

use cargo_credential::{Credential, Error};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME};
use windows_sys::Win32::Security::Credentials;

struct WindowsCredential;

/// Converts a string to a nul-terminated wide UTF-16 byte sequence.
fn wstr(s: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = OsStr::new(s).encode_wide().collect();
    if wide.iter().any(|b| *b == 0) {
        panic!("nul byte in wide string");
    }
    wide.push(0);
    wide
}

fn target_name(registry_name: &str) -> Vec<u16> {
    wstr(&format!("cargo-registry:{}", registry_name))
}

impl Credential for WindowsCredential {
    fn name(&self) -> &'static str {
        env!("CARGO_PKG_NAME")
    }

    fn get(&self, index_url: &str) -> Result<String, Error> {
        let target_name = target_name(index_url);
        let mut p_credential: *mut Credentials::CREDENTIALW = std::ptr::null_mut();
        unsafe {
            if Credentials::CredReadW(
                target_name.as_ptr(),
                Credentials::CRED_TYPE_GENERIC,
                0,
                &mut p_credential,
            ) == 0
            {
                return Err(
                    format!("failed to fetch token: {}", std::io::Error::last_os_error()).into(),
                );
            }
            let bytes = std::slice::from_raw_parts(
                (*p_credential).CredentialBlob,
                (*p_credential).CredentialBlobSize as usize,
            );
            String::from_utf8(bytes.to_vec()).map_err(|_| "failed to convert token to UTF8".into())
        }
    }

    fn store(&self, index_url: &str, token: &str, name: Option<&str>) -> Result<(), Error> {
        let token = token.as_bytes();
        let target_name = target_name(index_url);
        let comment = match name {
            Some(name) => wstr(&format!("Cargo registry token for {}", name)),
            None => wstr("Cargo registry token"),
        };
        let mut credential = Credentials::CREDENTIALW {
            Flags: 0,
            Type: Credentials::CRED_TYPE_GENERIC,
            TargetName: target_name.as_ptr() as *mut u16,
            Comment: comment.as_ptr() as *mut u16,
            LastWritten: FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: token.len() as u32,
            CredentialBlob: token.as_ptr() as *mut u8,
            Persist: Credentials::CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: std::ptr::null_mut(),
            TargetAlias: std::ptr::null_mut(),
            UserName: std::ptr::null_mut(),
        };
        let result = unsafe { Credentials::CredWriteW(&mut credential, 0) };
        if result == 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("failed to store token: {}", err).into());
        }
        Ok(())
    }

    fn erase(&self, index_url: &str) -> Result<(), Error> {
        let target_name = target_name(index_url);
        let result = unsafe {
            Credentials::CredDeleteW(target_name.as_ptr(), Credentials::CRED_TYPE_GENERIC, 0)
        };
        if result == 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
                eprintln!("not currently logged in to `{}`", index_url);
                return Ok(());
            }
            return Err(format!("failed to remove token: {}", err).into());
        }
        Ok(())
    }
}

fn main() {
    cargo_credential::main(WindowsCredential);
}
