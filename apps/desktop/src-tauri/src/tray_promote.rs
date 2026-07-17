//! Keep the Voice tray icon in the visible notification area on Windows 11+.
//! OS still owns icon order; apps cannot pin "next to the chevron".

#[cfg(windows)]
pub fn promote_voice_tray_icon_async() {
    std::thread::spawn(|| {
        // #region agent log
        crate::agent_debug_log(
            "D",
            "tray_promote.rs:async:enter",
            "tray promote worker started",
            serde_json::json!({}),
        );
        // #endregion
        // NotifyIconSettings entry appears shortly after Shell_NotifyIcon.
        for delay_ms in [0_u64, 400, 1200, 3000] {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
            // #region agent log
            let t0 = std::time::Instant::now();
            // #endregion
            let found = promote_voice_tray_icon();
            // #region agent log
            crate::agent_debug_log(
                "D",
                "tray_promote.rs:async:attempt",
                "tray promote attempt finished",
                serde_json::json!({
                    "delayMs": delay_ms,
                    "found": found,
                    "elapsedMs": t0.elapsed().as_millis() as u64,
                }),
            );
            // #endregion
            if found {
                break;
            }
        }
        // #region agent log
        crate::agent_debug_log(
            "D",
            "tray_promote.rs:async:exit",
            "tray promote worker done",
            serde_json::json!({}),
        );
        // #endregion
    });
}

#[cfg(not(windows))]
pub fn promote_voice_tray_icon_async() {}

#[cfg(windows)]
fn is_our_tray_executable(path: &str) -> bool {
    let normalized = path.replace('/', "\\").to_lowercase();
    if let Ok(exe) = std::env::current_exe() {
        if normalized == exe.to_string_lossy().replace('/', "\\").to_lowercase() {
            return true;
        }
    }
    let name = normalized.rsplit('\\').next().unwrap_or("");
    name == "voice-desktop.exe" || name == "voice.exe"
}

/// Sets `IsPromoted=1` for our NotifyIconSettings entries. Returns true if any match found.
#[cfg(windows)]
fn promote_voice_tray_icon() -> bool {
    use windows::core::{w, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_DWORD, RRF_RT_REG_SZ,
    };

    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\NotifyIconSettings"),
            Some(0),
            KEY_READ,
            &mut root,
        )
        .is_err()
        {
            return false;
        }

        let mut found = false;
        let mut index = 0_u32;
        loop {
            let mut name = [0_u16; 256];
            let mut name_len = name.len() as u32;
            let status = RegEnumKeyExW(
                root,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            );
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status != ERROR_SUCCESS {
                index += 1;
                continue;
            }

            let mut sub = HKEY::default();
            let open = RegOpenKeyExW(
                root,
                PCWSTR(name.as_ptr()),
                Some(0),
                KEY_READ | KEY_SET_VALUE,
                &mut sub,
            );
            if open.is_err() {
                index += 1;
                continue;
            }

            let mut exe_buf = [0_u16; 1024];
            let mut exe_bytes = (exe_buf.len() * 2) as u32;
            let mut value_type = REG_DWORD;
            let get = RegGetValueW(
                sub,
                None,
                w!("ExecutablePath"),
                RRF_RT_REG_SZ,
                Some(&mut value_type),
                Some(exe_buf.as_mut_ptr() as *mut _),
                Some(&mut exe_bytes),
            );

            if get.is_ok() {
                let exe_path = String::from_utf16_lossy(
                    &exe_buf[..exe_buf.iter().position(|&c| c == 0).unwrap_or(exe_buf.len())],
                );
                if is_our_tray_executable(&exe_path) {
                    let promoted: u32 = 1;
                    let _ = RegSetValueExW(
                        sub,
                        w!("IsPromoted"),
                        Some(0),
                        REG_DWORD,
                        Some(std::slice::from_raw_parts(
                            (&promoted as *const u32).cast::<u8>(),
                            std::mem::size_of::<u32>(),
                        )),
                    );
                    found = true;
                }
            }

            let _ = RegCloseKey(sub);
            index += 1;
        }

        let _ = RegCloseKey(root);
        found
    }
}
