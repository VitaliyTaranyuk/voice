//! Capture the focused text input at dictation start so inject targets that place.

use serde::Serialize;
use thiserror::Error;
use windows::core::BSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationValuePattern, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
    UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
    SetForegroundWindow, ShowWindow, GA_ROOT, SW_RESTORE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_MENU,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectStrategy {
    UiaText,
    UiaValue,
    FocusPaste,
    SendInput,
}

/// Snapshot of where text should land after ASR — immutable for the session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputTarget {
    /// Top-level window to restore.
    pub hwnd: isize,
    pub process_id: u32,
    pub runtime_id: Option<Vec<i32>>,
    pub can_insert: bool,
    pub strategy_hint: InjectStrategy,
    pub element_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum InputTargetError {
    #[error("no foreground window")]
    NoForeground,
    #[error("UI Automation error: {0}")]
    Uia(String),
    #[error("failed to restore target window focus")]
    FocusRestore,
}

pub fn capture_focused() -> Result<InputTarget, InputTargetError> {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() || !IsWindow(Some(foreground)).as_bool() {
            return Err(InputTargetError::NoForeground);
        }

        let hwnd = top_level_hwnd(foreground);
        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        ensure_com();

        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| InputTargetError::Uia(e.to_string()))?;

        let focused = automation
            .GetFocusedElement()
            .map_err(|e| InputTargetError::Uia(e.to_string()))?;

        let runtime_id = read_runtime_id(&automation, &focused);
        let element_name = focused.CurrentName().ok().map(|s| s.to_string());
        let (can_insert, strategy_hint) = classify_element(&focused);

        Ok(InputTarget {
            hwnd: hwnd.0 as isize,
            process_id,
            runtime_id,
            can_insert,
            strategy_hint,
            element_name,
        })
    }
}

/// Restore the captured window and focused element, then try UIA insert strategies.
pub fn prepare_target_for_inject(target: &InputTarget) -> Result<Option<IUIAutomationElement>, InputTargetError> {
    unsafe {
        let hwnd = HWND(target.hwnd as *mut _);
        if !IsWindow(Some(hwnd)).as_bool() {
            return Err(InputTargetError::FocusRestore);
        }

        force_foreground(hwnd)?;
        ensure_com();

        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| InputTargetError::Uia(e.to_string()))?;

        // Prefer element matching saved RuntimeId under the target window.
        if let Some(ref rid) = target.runtime_id {
            if let Ok(root) = automation.ElementFromHandle(hwnd) {
                if let Some(el) = find_by_runtime_id(&automation, &root, rid) {
                    let _ = el.SetFocus();
                    return Ok(Some(el));
                }
            }
        }

        if let Ok(focused) = automation.GetFocusedElement() {
            let _ = focused.SetFocus();
            return Ok(Some(focused));
        }

        if let Ok(root) = automation.ElementFromHandle(hwnd) {
            let _ = root.SetFocus();
            return Ok(Some(root));
        }

        Ok(None)
    }
}

pub fn try_uia_insert(element: &IUIAutomationElement, text: &str) -> Result<(), InputTargetError> {
    unsafe {
        // TextPattern presence signals a real text control; caret insert is done via paste after SetFocus.
        if element
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            .is_ok()
        {
            return Err(InputTargetError::Uia(
                "text pattern present — use focus+paste for caret insert".into(),
            ));
        }

        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            let current = pattern.CurrentValue().unwrap_or_default();
            if current.is_empty() {
                let bstr = BSTR::from(text);
                pattern
                    .SetValue(&bstr)
                    .map_err(|e| InputTargetError::Uia(e.to_string()))?;
                return Ok(());
            }
        }
    }
    Err(InputTargetError::Uia("no suitable UIA insert pattern".into()))
}

fn ensure_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

unsafe fn top_level_hwnd(hwnd: HWND) -> HWND {
    use windows::Win32::UI::WindowsAndMessaging::GetAncestor;
    let root = GetAncestor(hwnd, GA_ROOT);
    if !root.0.is_null() {
        root
    } else {
        hwnd
    }
}

unsafe fn force_foreground(hwnd: HWND) -> Result<(), InputTargetError> {
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }

    // Alt tap unlocks SetForegroundWindow on modern Windows.
    synth_alt_tap();

    let target_thread = GetWindowThreadProcessId(hwnd, None);
    let current_thread = GetCurrentThreadId();
    let attached = target_thread != 0 && target_thread != current_thread;
    if attached {
        let _ = AttachThreadInput(current_thread, target_thread, true);
    }

    let _ = BringWindowToTop(hwnd);
    let ok = SetForegroundWindow(hwnd).as_bool();

    if attached {
        let _ = AttachThreadInput(current_thread, target_thread, false);
    }

    if ok || foreground_is(hwnd) {
        return Ok(());
    }

    // Retry once after a short yield.
    thread_sleep_ms(30);
    synth_alt_tap();
    if SetForegroundWindow(hwnd).as_bool() || foreground_is(hwnd) {
        return Ok(());
    }

    Err(InputTargetError::FocusRestore)
}

unsafe fn foreground_is(hwnd: HWND) -> bool {
    let fg = GetForegroundWindow();
    if fg.0.is_null() {
        return false;
    }
    top_level_hwnd(fg) == hwnd || fg == hwnd
}

fn thread_sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

fn synth_alt_tap() {
    let inputs = [
        key_vk(VK_MENU, false),
        key_vk(VK_MENU, true),
    ];
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn key_vk(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

unsafe fn classify_element(element: &IUIAutomationElement) -> (bool, InjectStrategy) {
    let control_type = element.CurrentControlType().unwrap_or_default();
    let enabled = element.CurrentIsEnabled().unwrap_or(false.into()).as_bool();
    let focusable = element
        .CurrentIsKeyboardFocusable()
        .unwrap_or(false.into())
        .as_bool();

    let is_edit = control_type == UIA_EditControlTypeId
        || control_type == UIA_DocumentControlTypeId;

    if element
        .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        .is_ok()
    {
        return (enabled, InjectStrategy::UiaText);
    }

    if element
        .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        .is_ok()
        && (is_edit || focusable)
    {
        return (enabled && focusable, InjectStrategy::UiaValue);
    }

    if (is_edit || focusable) && enabled {
        return (true, InjectStrategy::FocusPaste);
    }

    (false, InjectStrategy::SendInput)
}

unsafe fn read_runtime_id(
    automation: &IUIAutomation,
    element: &IUIAutomationElement,
) -> Option<Vec<i32>> {
    let sa = element.GetRuntimeId().ok()?;
    if sa.is_null() {
        return None;
    }
    let mut ptr: *mut i32 = std::ptr::null_mut();
    let count = automation.IntSafeArrayToNativeArray(sa, &mut ptr).ok()?;
    if ptr.is_null() || count <= 0 {
        return None;
    }
    let ids = std::slice::from_raw_parts(ptr, count as usize).to_vec();
    windows::Win32::System::Com::CoTaskMemFree(Some(ptr as *const _));
    Some(ids)
}

unsafe fn find_by_runtime_id(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
    want: &[i32],
) -> Option<IUIAutomationElement> {
    if let Some(rid) = read_runtime_id(automation, root) {
        if rid == want {
            return Some(root.clone());
        }
    }

    // Breadth-limited walk: focused subtree is usually shallow for edit controls.
    let condition = automation.CreateTrueCondition().ok()?;
    let found = root
        .FindAll(
            windows::Win32::UI::Accessibility::TreeScope_Descendants,
            &condition,
        )
        .ok()?;
    let len = found.Length().ok()? as i32;
    let max = len.min(400);
    for i in 0..max {
        if let Ok(el) = found.GetElement(i) {
            if let Some(rid) = read_runtime_id(automation, &el) {
                if rid == want {
                    return Some(el);
                }
            }
        }
    }
    None
}
