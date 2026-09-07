//! Single-instance guard + local IPC for second-launch/deep-link forwarding.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

const IPC_ADDRESS: &str = "127.0.0.1:43827";

pub struct Claim {
    handle: windows::Win32::Foundation::HANDLE,
    receiver: Option<mpsc::Receiver<String>>,
}

/// Returns None if another instance is already running.
pub fn acquire() -> Option<Claim> {
    let name: Vec<u16> = "Global\\aitoolplus-single-instance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.ok()?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let _ = unsafe { CloseHandle(handle) };
        return None;
    }

    let listener = TcpListener::bind(IPC_ADDRESS).ok()?;
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("single-instance-ipc".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let mut reader = BufReader::new(stream);
                let mut message = String::new();
                if reader.read_line(&mut message).is_ok() {
                    let message = message.trim().to_string();
                    if !message.is_empty() && sender.send(message).is_err() {
                        break;
                    }
                }
            }
        })
        .ok()?;

    Some(Claim {
        handle,
        receiver: Some(receiver),
    })
}

impl Claim {
    pub fn take_receiver(&mut self) -> mpsc::Receiver<String> {
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

/// Poll second-instance messages and import recognized deep links.
pub fn pump_messages(
    receiver: mpsc::Receiver<String>,
    updater: crate::tray::TrayMenuUpdater,
    cx: &mut gpui::App,
) {
    cx.spawn(async move |cx| {
        loop {
            while let Ok(message) = receiver.try_recv() {
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
                                if message.starts_with("aitoolbox://") {
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
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
        }
    })
    .detach();
}

/// Register `aitoolbox://` for the current user (Windows, no elevation).
pub fn register_protocol() -> Result<(), String> {
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

    set_default("Software\\Classes\\aitoolbox", "URL:AI ToolPlus Protocol")?;
    set_default("Software\\Classes\\aitoolbox\\URL Protocol", "")?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    set_default(
        "Software\\Classes\\aitoolbox\\shell\\open\\command",
        &format!("\"{}\" \"%1\"", executable.display()),
    )
}

impl Drop for Claim {
    fn drop(&mut self) {
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
