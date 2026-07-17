use serde::Serialize;
use thiserror::Error;
use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppContext {
    pub app_id: String,
    pub app_category: String,
    pub window_title: Option<String>,
    pub process_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum ContextError {
    #[error("failed to detect foreground window")]
    DetectionFailed,
}

pub fn detect_foreground() -> Result<AppContext, ContextError> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return Err(ContextError::DetectionFailed);
        }

        let title = read_window_title(hwnd);
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process_name = if pid != 0 {
            read_process_name(pid)
        } else {
            None
        };

        let category = classify(process_name.as_deref(), title.as_deref());
        Ok(AppContext {
            app_id: process_name
                .clone()
                .unwrap_or_else(|| "unknown".into())
                .to_lowercase(),
            app_category: category.into(),
            window_title: title,
            process_name,
        })
    }
}

unsafe fn read_window_title(hwnd: HWND) -> Option<String> {
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return None;
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    let copied = GetWindowTextW(hwnd, &mut buf);
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..copied as usize]))
}

unsafe fn read_process_name(pid: u32) -> Option<String> {
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
    let mut buf = [0u16; MAX_PATH as usize];
    let len = GetModuleBaseNameW(handle, None, &mut buf);
    let _ = CloseHandle(handle);
    if len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

fn classify(process: Option<&str>, title: Option<&str>) -> &'static str {
    let p = process.unwrap_or("").to_lowercase();
    let t = title.unwrap_or("").to_lowercase();
    let hay = format!("{p} {t}");

    if hay.contains("cursor")
        || hay.contains("code")
        || hay.contains("idea")
        || hay.contains("webstorm")
        || hay.contains("pycharm")
        || hay.contains("devenv")
    {
        return "ide";
    }
    if hay.contains("slack")
        || hay.contains("discord")
        || hay.contains("telegram")
        || hay.contains("teams")
    {
        return "chat";
    }
    if hay.contains("outlook") || hay.contains("thunderbird") || hay.contains("mail") {
        return "email";
    }
    if hay.contains("chrome")
        || hay.contains("msedge")
        || hay.contains("firefox")
        || hay.contains("brave")
        || hay.contains("opera")
    {
        return "browser";
    }
    if hay.contains("winword")
        || hay.contains("notion")
        || hay.contains("obsidian")
        || hay.contains("word")
    {
        return "docs";
    }
    "other"
}
