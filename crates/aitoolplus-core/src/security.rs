//! Safe credential encryption and protection using Windows DPAPI.
//!
//! WebDAV passwords, S3 secret keys, and other sensitive credentials are
//! protected at rest with DPAPI (`CryptProtectData`), tying the ciphertext
//! to the local Windows user profile. When read, legacy plaintext passwords
//! and unprotected strings are parsed transparently for backward compatibility.

use base64::Engine;

const DPAPI_PREFIX: &str = "dpapi:";

/// Encrypt a plaintext secret using Windows DPAPI if running on Windows.
/// If already encrypted or empty, returns as-is.
pub fn protect_secret(plain: &str) -> String {
    if plain.is_empty() || plain.starts_with(DPAPI_PREFIX) {
        return plain.to_string();
    }
    #[cfg(target_os = "windows")]
    {
        match win_dpapi::protect(plain.as_bytes()) {
            Ok(cipher) => {
                let encoded = base64::engine::general_purpose::STANDARD.encode(cipher);
                format!("{DPAPI_PREFIX}{encoded}")
            }
            Err(e) => {
                tracing::warn!("DPAPI protection failed, keeping plaintext: {e}");
                plain.to_string()
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        plain.to_string()
    }
}

/// Decrypt a secret. If it starts with `dpapi:`, it is decrypted via DPAPI.
/// Otherwise, it is returned as plaintext (supporting legacy and hand-edited files).
pub fn unprotect_secret(stored: &str) -> String {
    if stored.is_empty() {
        return String::new();
    }
    if let Some(encoded) = stored.strip_prefix(DPAPI_PREFIX) {
        #[cfg(target_os = "windows")]
        {
            let cipher = match base64::engine::general_purpose::STANDARD.decode(encoded.trim()) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!("invalid base64 in DPAPI secret: {e}");
                    return String::new();
                }
            };
            match win_dpapi::unprotect(&cipher) {
                Ok(plain) => match String::from_utf8(plain) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("decrypted secret is not valid UTF-8: {e}");
                        String::new()
                    }
                },
                Err(e) => {
                    tracing::warn!("DPAPI decryption failed: {e}");
                    String::new()
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            stored.to_string()
        }
    } else {
        stored.to_string()
    }
}

#[cfg(target_os = "windows")]
mod win_dpapi {
    use std::ptr;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CryptProtectData(
            p_data_in: *const DataBlob,
            sz_data_descr: *const u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut std::ffi::c_void,
            p_prompt_struct: *mut std::ffi::c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;

        fn CryptUnprotectData(
            p_data_in: *const DataBlob,
            ppsz_data_descr: *mut *mut u16,
            p_optional_entropy: *const DataBlob,
            pv_reserved: *mut std::ffi::c_void,
            p_prompt_struct: *mut std::ffi::c_void,
            dw_flags: u32,
            p_data_out: *mut DataBlob,
        ) -> i32;

        fn LocalFree(h_mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    // CRYPTPROTECT_UI_FORBIDDEN = 0x1
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

    pub fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
        let in_blob = DataBlob {
            cb_data: data.len() as u32,
            pb_data: data.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };
        let result = unsafe {
            CryptProtectData(
                &in_blob,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        if result == 0 {
            return Err("CryptProtectData failed".into());
        }
        let out_bytes = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize).to_vec()
        };
        unsafe {
            LocalFree(out_blob.pb_data as *mut _);
        }
        Ok(out_bytes)
    }

    pub fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        let in_blob = DataBlob {
            cb_data: data.len() as u32,
            pb_data: data.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: ptr::null_mut(),
        };
        let result = unsafe {
            CryptUnprotectData(
                &in_blob,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        if result == 0 {
            return Err("CryptUnprotectData failed".into());
        }
        let out_bytes = unsafe {
            std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize).to_vec()
        };
        unsafe {
            LocalFree(out_blob.pb_data as *mut _);
        }
        Ok(out_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_protect_and_unprotect() {
        let secret = "secret_password_123!@#";
        let protected = protect_secret(secret);
        #[cfg(target_os = "windows")]
        {
            assert!(protected.starts_with(DPAPI_PREFIX));
            assert_ne!(protected, secret);
        }
        let unprotected = unprotect_secret(&protected);
        assert_eq!(unprotected, secret);
    }

    #[test]
    fn plaintext_is_backward_compatible() {
        let plain = "mypassword";
        assert_eq!(unprotect_secret(plain), plain);
    }

    #[test]
    fn empty_handling() {
        assert_eq!(protect_secret(""), "");
        assert_eq!(unprotect_secret(""), "");
    }
}
