//! Single-instance guard + local IPC for second-launch/deep-link forwarding.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};


#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
#[cfg(windows)]
use windows::Win32::System::Threading::CreateMutexW;
#[cfg(windows)]
use windows::core::PCWSTR;

const IPC_ADDRESS: &str = "127.0.0.1:43827";

pub struct Claim {
    #[cfg(windows)]
    handle: windows::Win32::Foundation::HANDLE,
    receiver: Option<async_channel::Receiver<String>>,
}

/// Returns None if another instance is already running.
pub fn acquire() -> Option<Claim> {
    #[cfg(windows)]
    let handle = {
        let name: Vec<u16> = "Global\\aitoolplus-single-instance"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.ok()?;
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            let _ = unsafe { CloseHandle(handle) };
            return None;
        }
        handle
    };

    let listener = TcpListener::bind(IPC_ADDRESS).ok()?;
    let (sender, receiver) = async_channel::unbounded();
    std::thread::Builder::new()
        .name("single-instance-ipc".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let mut reader = BufReader::new(stream);
                let mut message = String::new();
                if reader.read_line(&mut message).is_ok() {
                    let message = message.trim().to_string();
                    if !message.is_empty() && sender.send_blocking(message).is_err() {
                        break;
                    }
                }
            }
        })
        .ok()?;

    Some(Claim {
        #[cfg(windows)]
        handle,
        receiver: Some(receiver),
    })
}

impl Claim {
    pub fn take_receiver(&mut self) -> async_channel::Receiver<String> {
        self.receiver
            .take()
            .expect("instance receiver already taken")
    }
}

/// Forward an argv/deep-link payload to the running instance.
pub fn forward_to_existing(message: &str) -> Result<(), String> {
    let mut stream = TcpStream::connect_timeout(
        &IPC_ADDRESS
            .parse()
            .map_err(|e| format!("invalid IPC address: {e}"))?,
        std::time::Duration::from_secs(2),
    )
    .map_err(|e| format!("connect to running instance failed: {e}"))?;
    stream
        .write_all(format!("{message}\n").as_bytes())
        .map_err(|e| e.to_string())
}

/// Import deep links and activation messages from later launches.
pub fn pump_messages(
    receiver: async_channel::Receiver<String>,
    updater: crate::tray::TrayMenuUpdater,
    cx: &mut gpui::App,
) {
    // The IPC thread blocks in accept/read. This await does not hold a pool thread.
    cx.spawn(async move |cx| {
        while let Ok(message) = receiver.recv().await {
                let handle = cx.update(|cx| {
                    cx.windows()
                        .into_iter()
                        .find_map(|window| window.downcast::<gpui_kit::component::Root>())
                });
                let handle = match handle {
                    Some(handle) => Some(handle),
                    None => cx.update(|cx| {
                        let mut application = crate::app::App::load().ok()?;
                        crate::app::open_main_window(&mut application, updater.clone(), cx).ok()
                    }),
                };
                if let Some(handle) = handle {
                    let _ = handle.update(cx, |root, window, cx| {
                        if let Ok(workspace) = root.view().clone().downcast::<aitoolplus_ui::Workspace>() {
                            workspace.update(cx, |workspace, cx| {
                                if message.starts_with("aitoolplus://")
                                    || message.starts_with("aitoolbox://")
                                    || message.starts_with("ccswitch://")
                                {
                                    match aitoolplus_core::deeplink::import_into_store(
                                        &message,
                                        workspace.store.store_mut(),
                                    ) {
                                        Ok((tool, name)) => {
                                            workspace.persist_store();
                                            workspace.navigate(aitoolplus_ui::pages::Page::Tool(tool), cx);
                                            workspace.ui.toast(
                                                workspace
                                                    .i18n
                                                    .t(
                                                        &format!("已导入供应商 {name}"),
                                                        &format!("imported provider {name}"),
                                                    )
                                                    .to_string(),
                                                false,
                                            );
                                        }
                                        Err(error) => workspace.ui.toast(error, true),
                                    }
                                }
                                cx.notify();
                            });
                        }
                        window.activate_window();
                    });
                }
        }
    })
    .detach();
}

/// Register `aitoolbox://` for the current user (Windows, no elevation).
pub fn register_protocol() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
            RegCreateKeyExW, RegSetValueExW,
        };

        fn wide(value: &str) -> Vec<u16> {
            value.encode_utf16().chain(std::iter::once(0)).collect()
        }
        fn set_default(subkey: &str, value: &str) -> Result<(), String> {
            let subkey = wide(subkey);
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
                return Err(format!("create protocol key failed: {}", status.0));
            }
            let value = wide(value);
            let bytes =
                unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
            let status = unsafe { RegSetValueExW(key, PCWSTR::null(), None, REG_SZ, Some(bytes)) };
            let _ = unsafe { RegCloseKey(key) };
            if status == windows::Win32::Foundation::ERROR_SUCCESS {
                Ok(())
            } else {
                Err(format!("write protocol key failed: {}", status.0))
            }
        }

        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let cmd = format!("\"{}\" \"%1\"", executable.display());

        // Register aitoolplus:// (primary)
        set_default("Software\\Classes\\aitoolplus", "URL:AI ToolPlus Protocol")?;
        set_default("Software\\Classes\\aitoolplus\\URL Protocol", "")?;
        set_default(
            "Software\\Classes\\aitoolplus\\shell\\open\\command",
            &cmd,
        )?;

        // Register aitoolbox:// (legacy compatibility)
        set_default("Software\\Classes\\aitoolbox", "URL:AI ToolPlus Protocol")?;
        set_default("Software\\Classes\\aitoolbox\\URL Protocol", "")?;
        set_default(
            "Software\\Classes\\aitoolbox\\shell\\open\\command",
            &cmd,
        )?;

        Ok(())
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarding_uses_line_protocol() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            line
        });
        let mut stream = TcpStream::connect(address).unwrap();
        stream.write_all(b"aitoolbox://v1/import?x=1\n").unwrap();
        assert_eq!(server.join().unwrap().trim(), "aitoolbox://v1/import?x=1");
    }
}
