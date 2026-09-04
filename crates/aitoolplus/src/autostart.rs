//! Windows current-user autostart via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//!
//! This is per-user, requires no elevation, and mirrors ordinary desktop
//! switcher behavior. Non-Windows builds return false/no-op.

const VALUE_NAME: &str = "AIToolPlus";

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Enable or disable launch-at-login for the current executable.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
            RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
        };
        use windows::core::PCWSTR;

        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = wide(VALUE_NAME);
        let mut key = windows::Win32::System::Registry::HKEY::default();
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
        };
        if status != windows::Win32::Foundation::ERROR_SUCCESS {
            return Err(format!("open HKCU Run failed: {}", status.0));
        }

        let result = if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let command = wide(&format!("\"{}\"", exe.display()));
            let bytes = unsafe {
                std::slice::from_raw_parts(command.as_ptr().cast::<u8>(), command.len() * 2)
            };
            unsafe { RegSetValueExW(key, PCWSTR(value_name.as_ptr()), None, REG_SZ, Some(bytes)) }
        } else {
            let status = unsafe { RegDeleteValueW(key, PCWSTR(value_name.as_ptr())) };
            // Missing value is already disabled.
            if status == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                windows::Win32::Foundation::ERROR_SUCCESS
            } else {
                status
            }
        };
        let _ = unsafe { RegCloseKey(key) };
        if result == windows::Win32::Foundation::ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("update HKCU Run failed: {}", result.0))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Ok(())
    }
}

/// Whether the current-user Run value exists.
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_QUERY_VALUE, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
        };
        use windows::core::PCWSTR;

        let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = wide(VALUE_NAME);
        let mut key = windows::Win32::System::Registry::HKEY::default();
        let open = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                None,
                KEY_QUERY_VALUE,
                &mut key,
            )
        };
        if open != windows::Win32::Foundation::ERROR_SUCCESS {
            return false;
        }
        let query =
            unsafe { RegQueryValueExW(key, PCWSTR(value_name.as_ptr()), None, None, None, None) };
        let _ = unsafe { RegCloseKey(key) };
        query == windows::Win32::Foundation::ERROR_SUCCESS
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_name_is_stable() {
        assert_eq!(VALUE_NAME, "AIToolPlus");
    }
}
