#![windows_subsystem = "windows"]

mod sqlite;

use serde_json::Value;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use windows_sys::Win32::Foundation::{COLORREF, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
    DEFAULT_QUALITY, DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteObject,
    DrawTextW, FF_DONTCARE, FW_NORMAL, FillRect, GetDC, HBRUSH, HFONT, InvalidateRect,
    OUT_DEFAULT_PRECIS, ReleaseDC, SelectObject, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
    UpdateWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    DRAWITEMSTRUCT, EM_SETSEL, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
    ODS_SELECTED,
};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_NEXT, VK_PRIOR,
    VK_R, VK_RETURN, VK_SHIFT, VK_UP,
};
use windows_sys::Win32::UI::Shell::{
    DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const ID_FILTER: usize = 100;
const ID_LIST: usize = 101;
const ID_REMOTE: usize = 102;
const ID_FOOTER: usize = 103;
const ID_FOOTER_SEPARATOR: usize = 104;
const WM_ENTRIES_LOADED: u32 = WM_APP + 1;
const WM_LAUNCH_SELECTED: u32 = WM_APP + 2;
const SS_CENTERIMAGE: u32 = 0x0000_0200;
const SS_ENDELLIPSIS: u32 = 0x0000_4000;
const SS_ETCHEDHORZ: u32 = 0x0000_0010;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const RRF_RT_REG_DWORD: u32 = 0x0000_0018;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        key: *mut c_void,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        value_type: *mut u32,
        data: *mut c_void,
        data_size: *mut u32,
    ) -> i32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(
        window: HWND,
        attribute: u32,
        value: *const c_void,
        value_size: u32,
    ) -> i32;
}

#[link(name = "uxtheme")]
unsafe extern "system" {
    fn SetWindowTheme(window: HWND, sub_app_name: *const u16, sub_id_list: *const u16) -> i32;
}

#[derive(Clone, Debug)]
struct Entry {
    uri: String,
    label: String,
    remote: String,
    remote_key: String,
    search_key: String,
}

struct AppState {
    filter: HWND,
    remote_filter: HWND,
    list: HWND,
    footer: HWND,
    footer_separator: HWND,
    entries: Vec<Entry>,
    shown: Vec<usize>,
    remote_keys: Vec<Option<String>>,
    load_error: Option<String>,
    alive: Arc<AtomicBool>,
    font: HFONT,
    footer_font: HFONT,
    theme: Theme,
    window_brush: HBRUSH,
    control_brush: HBRUSH,
}

#[derive(Clone, Copy)]
struct Theme {
    dark: bool,
    window: COLORREF,
    control: COLORREF,
    text: COLORREF,
    muted: COLORREF,
}

impl Theme {
    fn light() -> Self {
        Self {
            dark: false,
            window: rgb(245, 245, 245),
            control: rgb(255, 255, 255),
            text: rgb(25, 25, 28),
            muted: rgb(96, 96, 96),
        }
    }

    fn dark() -> Self {
        Self {
            dark: true,
            window: rgb(30, 30, 30),
            control: rgb(37, 37, 38),
            text: rgb(232, 232, 232),
            muted: rgb(165, 165, 165),
        }
    }
}

static CLASS_NAME: &[u16] = &[
    b'V' as u16,
    b'S' as u16,
    b'R' as u16,
    b'e' as u16,
    b'c' as u16,
    b'e' as u16,
    b'n' as u16,
    b't' as u16,
    b'W' as u16,
    b'i' as u16,
    b'n' as u16,
    b'd' as u16,
    b'o' as u16,
    b'w' as u16,
    0,
];

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main() {
    if let Err(error) = run() {
        show_error(std::ptr::null_mut(), &error);
    }
}

fn run() -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        });
        let arguments: Vec<String> = std::env::args().collect();
        let theme = requested_theme(&arguments).unwrap_or_else(system_theme);
        let instance = GetModuleHandleW(std::ptr::null());
        let cursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        let icon = LoadIconW(instance, 1 as *const u16);
        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hIcon: icon,
            hCursor: cursor,
            hbrBackground: std::ptr::null_mut(),
            lpszClassName: CLASS_NAME.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&window_class) == 0 {
            return Err(format!("could not register window ({})", GetLastError()));
        }

        let alive = Arc::new(AtomicBool::new(true));
        let state = Box::new(AppState {
            filter: std::ptr::null_mut(),
            remote_filter: std::ptr::null_mut(),
            list: std::ptr::null_mut(),
            footer: std::ptr::null_mut(),
            footer_separator: std::ptr::null_mut(),
            entries: Vec::new(),
            shown: Vec::new(),
            remote_keys: Vec::new(),
            load_error: None,
            alive: Arc::clone(&alive),
            font: std::ptr::null_mut(),
            footer_font: std::ptr::null_mut(),
            theme,
            window_brush: CreateSolidBrush(theme.window),
            control_brush: CreateSolidBrush(theme.control),
        });
        let title = wide("VS Recent");
        let window = CreateWindowExW(
            WS_EX_APPWINDOW,
            CLASS_NAME.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            620,
            500,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            Box::into_raw(state).cast(),
        );
        if window.is_null() {
            return Err(format!("could not create window ({})", GetLastError()));
        }

        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        SetForegroundWindow(window);

        let demo = arguments
            .iter()
            .any(|argument| argument.eq_ignore_ascii_case("--demo"));
        let window_address = window as usize;
        std::thread::spawn(move || {
            let window = window_address as HWND;
            let loaded = if demo {
                Ok(demo_entries())
            } else {
                load_entries()
            };
            let payload = Box::into_raw(Box::new(loaded));
            if !alive.load(Ordering::Acquire)
                || PostMessageW(window, WM_ENTRIES_LOADED, 0, payload as LPARAM) == 0
            {
                drop(Box::from_raw(payload));
            }
        });

        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            if handle_picker_key(window, &message) || IsDialogMessageW(window, &message) != 0 {
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

fn requested_theme(arguments: &[String]) -> Option<Theme> {
    if arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--dark"))
    {
        Some(Theme::dark())
    } else if arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--light"))
    {
        Some(Theme::light())
    } else {
        None
    }
}

fn system_theme() -> Theme {
    let mut light_theme = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            0x8000_0001usize as *mut c_void,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut light_theme as *mut u32).cast(),
            &mut size,
        )
    };
    if result == 0 && light_theme == 0 {
        Theme::dark()
    } else {
        Theme::light()
    }
}

unsafe fn handle_picker_key(window: HWND, message: &MSG) -> bool {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return false;
    }
    let (filter, remote_filter, list) =
        unsafe { ((*pointer).filter, (*pointer).remote_filter, (*pointer).list) };

    if message.message == WM_SYSKEYDOWN && message.wParam as u16 == VK_R {
        unsafe {
            SetFocus(remote_filter);
            SendMessageW(remote_filter, CB_SHOWDROPDOWN, 1, 0);
        }
        return true;
    }
    if message.message != WM_KEYDOWN {
        return false;
    }
    if message.hwnd != filter && message.hwnd != list {
        return false;
    }

    match message.wParam as u16 {
        VK_RETURN => {
            unsafe { PostMessageW(window, WM_LAUNCH_SELECTED, 0, 0) };
            true
        }
        VK_ESCAPE => {
            unsafe { DestroyWindow(window) };
            true
        }
        VK_DOWN => {
            move_selection(list, 1);
            true
        }
        VK_UP => {
            move_selection(list, -1);
            true
        }
        VK_NEXT => {
            move_selection(list, 8);
            true
        }
        VK_PRIOR => {
            move_selection(list, -8);
            true
        }
        VK_HOME if unsafe { GetKeyState(VK_CONTROL as i32) } < 0 => {
            unsafe { SendMessageW(list, LB_SETCURSEL, 0, 0) };
            true
        }
        VK_END if unsafe { GetKeyState(VK_CONTROL as i32) } < 0 => {
            let count = unsafe { SendMessageW(list, LB_GETCOUNT, 0, 0) };
            if count > 0 {
                unsafe { SendMessageW(list, LB_SETCURSEL, (count - 1) as WPARAM, 0) };
            }
            true
        }
        _ => false,
    }
}

fn move_selection(list: HWND, delta: isize) {
    unsafe {
        let count = SendMessageW(list, LB_GETCOUNT, 0, 0);
        if count <= 0 {
            return;
        }
        let current = SendMessageW(list, LB_GETCURSEL, 0, 0);
        let current = if current == LB_ERR as isize {
            0
        } else {
            current
        };
        let next = (current + delta).clamp(0, count - 1);
        SendMessageW(list, LB_SETCURSEL, next as WPARAM, 0);
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        WM_CREATE => {
            unsafe { create_controls(window) };
            unsafe { apply_theme(window) };
            unsafe { install_filter_placeholder(window) };
            0
        }
        WM_ERASEBKGND => {
            let pointer = unsafe { state_ptr(window) };
            if pointer.is_null() {
                return unsafe { DefWindowProcW(window, message, wparam, lparam) };
            }
            let mut bounds = RECT::default();
            unsafe {
                GetClientRect(window, &mut bounds);
                FillRect(wparam as _, &bounds, (*pointer).window_brush);
            }
            1
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let pointer = unsafe { state_ptr(window) };
            if pointer.is_null() {
                return unsafe { DefWindowProcW(window, message, wparam, lparam) };
            }
            unsafe {
                SetTextColor(wparam as _, (*pointer).theme.text);
                SetBkColor(wparam as _, (*pointer).theme.control);
                (*pointer).control_brush as LRESULT
            }
        }
        WM_CTLCOLORSTATIC => {
            let pointer = unsafe { state_ptr(window) };
            if pointer.is_null() {
                return unsafe { DefWindowProcW(window, message, wparam, lparam) };
            }
            unsafe {
                SetTextColor(wparam as _, (*pointer).theme.muted);
                SetBkColor(wparam as _, (*pointer).theme.control);
                (*pointer).control_brush as LRESULT
            }
        }
        WM_SIZE => {
            let pointer = unsafe { state_ptr(window) };
            if !pointer.is_null() {
                let width = (lparam as u32 & 0xffff) as i32;
                let height = ((lparam as u32 >> 16) & 0xffff) as i32;
                unsafe { layout_controls(window, width, height) };
            }
            0
        }
        WM_DPICHANGED => {
            let suggested = unsafe { &*(lparam as *const RECT) };
            unsafe {
                SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                update_row_height(window);
                update_control_font(window);
            }
            0
        }
        WM_COMMAND => {
            let id = wparam & 0xffff;
            let notification = (wparam >> 16) & 0xffff;
            if id == ID_FILTER && notification == EN_CHANGE as usize {
                unsafe { apply_filter(window) };
            } else if id == ID_LIST && notification == LBN_DBLCLK as usize {
                unsafe { launch_selected(window) };
            } else if id == ID_LIST && notification == LBN_SELCHANGE as usize {
                let pointer = unsafe { state_ptr(window) };
                if !pointer.is_null() {
                    unsafe { SetFocus((*pointer).filter) };
                }
            } else if id == ID_REMOTE && notification == CBN_SELCHANGE as usize {
                unsafe { apply_filter(window) };
            }
            0
        }
        WM_DRAWITEM => {
            if wparam == ID_LIST {
                unsafe { draw_list_row(window, lparam) };
                1
            } else {
                unsafe { DefWindowProcW(window, message, wparam, lparam) }
            }
        }
        WM_ENTRIES_LOADED => {
            let payload = unsafe { Box::from_raw(lparam as *mut Result<Vec<Entry>, String>) };
            match *payload {
                Ok(entries) => unsafe {
                    let pointer = state_ptr(window);
                    if !pointer.is_null() {
                        (*pointer).entries = entries;
                        (*pointer).load_error = None;
                    }
                    populate_remote_filter(window);
                    apply_filter(window);
                },
                Err(error) => unsafe {
                    let pointer = state_ptr(window);
                    if !pointer.is_null() {
                        (*pointer).load_error = Some(format!("Unable to load history: {error}"));
                    }
                    apply_filter(window);
                },
            }
            0
        }
        WM_LAUNCH_SELECTED => {
            unsafe { launch_selected(window) };
            0
        }
        WM_SETFOCUS => {
            let pointer = unsafe { state_ptr(window) };
            if !pointer.is_null() {
                unsafe { SetFocus((*pointer).filter) };
            }
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        WM_NCDESTROY => {
            let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut AppState };
            if !pointer.is_null() {
                unsafe {
                    (*pointer).alive.store(false, Ordering::Release);
                    if !(*pointer).font.is_null() {
                        DeleteObject((*pointer).font);
                    }
                    if !(*pointer).footer_font.is_null() {
                        DeleteObject((*pointer).footer_font);
                    }
                    DeleteObject((*pointer).window_brush);
                    DeleteObject((*pointer).control_brush);
                    SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                    drop(Box::from_raw(pointer));
                }
            }
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

unsafe fn create_controls(window: HWND) {
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let edit_class = wide("EDIT");
    let list_class = wide("LISTBOX");
    let combo_class = wide("COMBOBOX");
    let static_class = wide("STATIC");
    let loading = wide("Loading recent folders...");
    let empty = wide("");

    let filter = unsafe {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            edit_class.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            10,
            10,
            400,
            32,
            window,
            ID_FILTER as _,
            instance,
            std::ptr::null(),
        )
    };
    let remote_filter = unsafe {
        CreateWindowExW(
            0,
            combo_class.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
            420,
            10,
            170,
            300,
            window,
            ID_REMOTE as _,
            instance,
            std::ptr::null(),
        )
    };
    let list = unsafe {
        CreateWindowExW(
            WS_EX_CLIENTEDGE,
            list_class.as_ptr(),
            loading.as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | LBS_NOTIFY as u32
                | LBS_NOINTEGRALHEIGHT as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32,
            10,
            50,
            580,
            400,
            window,
            ID_LIST as _,
            instance,
            std::ptr::null(),
        )
    };
    let footer = unsafe {
        CreateWindowExW(
            0,
            static_class.as_ptr(),
            wide(
                "Up/Down Select | Enter Open | Ctrl+Enter New | Shift+Enter Keep | Alt+R Remote | Esc Close",
            )
            .as_ptr(),
            WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE | SS_ENDELLIPSIS,
            10,
            455,
            580,
            25,
            window,
            ID_FOOTER as _,
            instance,
            std::ptr::null(),
        )
    };
    let footer_separator = unsafe {
        CreateWindowExW(
            0,
            static_class.as_ptr(),
            empty.as_ptr(),
            WS_CHILD | WS_VISIBLE | SS_ETCHEDHORZ,
            10,
            454,
            580,
            1,
            window,
            ID_FOOTER_SEPARATOR as _,
            instance,
            std::ptr::null(),
        )
    };
    unsafe {
        SendMessageW(list, LB_ADDSTRING, 0, loading.as_ptr() as LPARAM);
        SendMessageW(
            remote_filter,
            CB_ADDSTRING,
            0,
            wide("All remotes").as_ptr() as LPARAM,
        );
        SendMessageW(remote_filter, CB_SETCURSEL, 0, 0);
        SetFocus(filter);
    }
    let pointer = unsafe { state_ptr(window) };
    if !pointer.is_null() {
        unsafe {
            (*pointer).filter = filter;
            (*pointer).remote_filter = remote_filter;
            (*pointer).list = list;
            (*pointer).footer = footer;
            (*pointer).footer_separator = footer_separator;
            (*pointer).remote_keys = vec![None];
        }
    }
    unsafe { update_row_height(window) };
    unsafe { update_control_font(window) };
}

unsafe fn install_filter_placeholder(window: HWND) {
    let pointer = unsafe { state_ptr(window) };
    if !pointer.is_null() {
        unsafe { SetWindowSubclass((*pointer).filter, Some(filter_proc), 1, window as usize) };
    }
}

unsafe extern "system" fn filter_proc(
    edit: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    parent_data: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        unsafe { RemoveWindowSubclass(edit, Some(filter_proc), subclass_id) };
    }
    if message == WM_CHAR && wparam == 1 {
        unsafe { SendMessageW(edit, EM_SETSEL, 0, -1) };
        return 0;
    }
    let result = unsafe { DefSubclassProc(edit, message, wparam, lparam) };
    if message == WM_PAINT && unsafe { GetWindowTextLengthW(edit) } == 0 {
        let pointer = unsafe { state_ptr(parent_data as HWND) };
        if !pointer.is_null() {
            let dc = unsafe { GetDC(edit) };
            if !dc.is_null() {
                let mut bounds = RECT::default();
                unsafe { GetClientRect(edit, &mut bounds) };
                let dpi = unsafe { GetDpiForWindow(edit) } as i32;
                bounds.left += (4 * dpi / 96).max(3);
                let old_font = unsafe { SelectObject(dc, (*pointer).font) };
                unsafe {
                    SetBkMode(dc, TRANSPARENT as i32);
                    SetTextColor(dc, (*pointer).theme.muted);
                    let placeholder = wide("Type to filter");
                    DrawTextW(
                        dc,
                        placeholder.as_ptr(),
                        placeholder.len().saturating_sub(1) as i32,
                        &mut bounds,
                        DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
                    );
                    SelectObject(dc, old_font);
                    ReleaseDC(edit, dc);
                }
            }
        }
    }
    result
}

unsafe fn apply_theme(window: HWND) {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return;
    }
    let dark = unsafe { (*pointer).theme.dark };
    let dark_value = i32::from(dark);
    let control_theme = wide(if dark {
        "DarkMode_Explorer"
    } else {
        "Explorer"
    });
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark_value as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        SetWindowTheme((*pointer).filter, control_theme.as_ptr(), std::ptr::null());
        SetWindowTheme(
            (*pointer).remote_filter,
            control_theme.as_ptr(),
            std::ptr::null(),
        );
        SetWindowTheme((*pointer).list, control_theme.as_ptr(), std::ptr::null());
        InvalidateRect(window, std::ptr::null(), 1);
    }
}

unsafe fn layout_controls(window: HWND, width: i32, height: i32) {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return;
    }
    let dpi = unsafe { GetDpiForWindow(window) } as i32;
    let margin = (10 * dpi / 96).max(8);
    let gap = (8 * dpi / 96).max(6);
    let control_height = (32 * dpi / 96).max(28);
    let remote_width = (170 * dpi / 96).max(140);
    let list_top = margin + control_height + gap;
    let footer_height = (25 * dpi / 96).max(22);
    let footer_top = height - margin - footer_height;
    unsafe {
        MoveWindow(
            (*pointer).filter,
            margin,
            margin,
            (width - margin * 2 - gap - remote_width).max(80),
            control_height,
            1,
        );
        MoveWindow(
            (*pointer).remote_filter,
            width - margin - remote_width,
            margin,
            remote_width,
            control_height * 10,
            1,
        );
        MoveWindow(
            (*pointer).list,
            margin,
            list_top,
            width - margin * 2,
            footer_top - list_top - gap,
            1,
        );
        MoveWindow(
            (*pointer).footer_separator,
            margin,
            footer_top,
            width - margin * 2,
            1,
            1,
        );
        MoveWindow(
            (*pointer).footer,
            margin,
            footer_top + 1,
            width - margin * 2,
            footer_height - 1,
            1,
        );
    }
}

unsafe fn update_row_height(window: HWND) {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() || unsafe { (*pointer).list.is_null() } {
        return;
    }
    let row_height = (30 * unsafe { GetDpiForWindow(window) } as i32 / 96).max(24);
    unsafe { SendMessageW((*pointer).list, LB_SETITEMHEIGHT, 0, row_height as LPARAM) };
}

unsafe fn update_control_font(window: HWND) {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return;
    }
    let dpi = unsafe { GetDpiForWindow(window) } as i32;
    let font = unsafe {
        CreateFontW(
            -(10 * dpi / 72),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            DEFAULT_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            wide("Segoe UI").as_ptr(),
        )
    };
    if font.is_null() {
        return;
    }
    let footer_font = unsafe {
        CreateFontW(
            -(750 * dpi / 7200),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            DEFAULT_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            wide("Segoe UI").as_ptr(),
        )
    };
    if footer_font.is_null() {
        unsafe { DeleteObject(font) };
        return;
    }
    unsafe {
        SendMessageW((*pointer).filter, WM_SETFONT, font as WPARAM, 1);
        SendMessageW((*pointer).remote_filter, WM_SETFONT, font as WPARAM, 1);
        SendMessageW((*pointer).list, WM_SETFONT, font as WPARAM, 1);
        SendMessageW((*pointer).footer, WM_SETFONT, footer_font as WPARAM, 1);
        if !(*pointer).font.is_null() {
            DeleteObject((*pointer).font);
        }
        if !(*pointer).footer_font.is_null() {
            DeleteObject((*pointer).footer_font);
        }
        (*pointer).font = font;
        (*pointer).footer_font = footer_font;
    }
}

unsafe fn state_ptr(window: HWND) -> *mut AppState {
    unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) as *mut AppState }
}

unsafe fn draw_list_row(window: HWND, lparam: LPARAM) {
    let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
    if draw.itemID == u32::MAX {
        return;
    }
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return;
    }

    let item_index = draw.itemID as usize;
    let remote = unsafe {
        let state = &*pointer;
        state
            .shown
            .get(item_index)
            .and_then(|entry_index| state.entries.get(*entry_index))
            .map(|entry| entry.remote.as_str())
    };
    let theme = unsafe { (*pointer).theme };
    let (normal, selected) = row_colors(remote.unwrap_or(""), theme.dark);
    let is_selected = draw.itemState & ODS_SELECTED != 0;
    let background = if is_selected { selected } else { normal };
    let foreground = if is_selected {
        rgb(255, 255, 255)
    } else {
        theme.text
    };
    let brush = unsafe { CreateSolidBrush(background) };
    unsafe {
        FillRect(draw.hDC, &draw.rcItem, brush);
        DeleteObject(brush);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(draw.hDC, foreground);
    }

    let text = list_item_text(draw.hwndItem, item_index);
    let mut text_bounds = draw.rcItem;
    let dpi = unsafe { GetDpiForWindow(window) } as i32;
    text_bounds.left += (10 * dpi / 96).max(8);
    text_bounds.right -= 8;
    let old_font = unsafe {
        if (*pointer).font.is_null() {
            std::ptr::null_mut()
        } else {
            SelectObject(draw.hDC, (*pointer).font)
        }
    };
    unsafe {
        DrawTextW(
            draw.hDC,
            text.as_ptr(),
            text.len().saturating_sub(1) as i32,
            &mut text_bounds,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        if !old_font.is_null() {
            SelectObject(draw.hDC, old_font);
        }
    }
}

fn list_item_text(list: HWND, index: usize) -> Vec<u16> {
    unsafe {
        let length = SendMessageW(list, LB_GETTEXTLEN, index, 0);
        if length == LB_ERR as isize || length < 0 {
            return wide("");
        }
        let mut text = vec![0u16; length as usize + 1];
        SendMessageW(list, LB_GETTEXT, index, text.as_mut_ptr() as LPARAM);
        text
    }
}

fn row_colors(remote: &str, dark: bool) -> (COLORREF, COLORREF) {
    let remote = remote.to_ascii_lowercase();
    if dark && remote == "local" {
        (rgb(38, 40, 45), rgb(79, 86, 99))
    } else if dark && remote.starts_with("wsl") {
        (rgb(57, 37, 31), rgb(194, 70, 25))
    } else if dark && remote.starts_with("ssh") {
        (rgb(31, 44, 61), rgb(31, 92, 173))
    } else if dark && remote.contains("container") {
        (rgb(29, 48, 56), rgb(25, 119, 177))
    } else if dark && (remote == "codespace" || remote == "github") {
        (rgb(45, 36, 61), rgb(103, 63, 168))
    } else if dark && remote == "tunnel" {
        (rgb(28, 52, 49), rgb(0, 116, 104))
    } else if dark {
        let hash = remote.bytes().fold(2_166_136_261u32, |value, byte| {
            (value ^ byte as u32).wrapping_mul(16_777_619)
        });
        let palette = [
            (rgb(58, 43, 27), rgb(166, 91, 0)),
            (rgb(31, 53, 37), rgb(42, 120, 63)),
            (rgb(55, 34, 42), rgb(153, 60, 91)),
            (rgb(36, 40, 61), rgb(67, 79, 164)),
        ];
        palette[hash as usize % palette.len()]
    } else if remote == "local" {
        (rgb(239, 241, 245), rgb(79, 86, 99))
    } else if remote.starts_with("wsl") {
        (rgb(255, 238, 229), rgb(194, 70, 25))
    } else if remote.starts_with("ssh") {
        (rgb(232, 241, 255), rgb(31, 92, 173))
    } else if remote.contains("container") {
        (rgb(228, 246, 255), rgb(25, 119, 177))
    } else if remote == "codespace" || remote == "github" {
        (rgb(241, 234, 255), rgb(103, 63, 168))
    } else if remote == "tunnel" {
        (rgb(227, 247, 242), rgb(0, 116, 104))
    } else {
        let hash = remote.bytes().fold(2_166_136_261u32, |value, byte| {
            (value ^ byte as u32).wrapping_mul(16_777_619)
        });
        let palette = [
            (rgb(255, 241, 223), rgb(166, 91, 0)),
            (rgb(232, 246, 235), rgb(42, 120, 63)),
            (rgb(250, 233, 239), rgb(153, 60, 91)),
            (rgb(235, 239, 255), rgb(67, 79, 164)),
        ];
        palette[hash as usize % palette.len()]
    }
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

unsafe fn apply_filter(window: HWND) {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return;
    }
    let (filter, remote_filter, list) =
        unsafe { ((*pointer).filter, (*pointer).remote_filter, (*pointer).list) };
    let query = control_text(filter).to_lowercase();
    let tokens: Vec<&str> = query.split_whitespace().collect();
    let remote_index = unsafe { SendMessageW(remote_filter, CB_GETCURSEL, 0, 0) };
    let selected_remote = unsafe {
        if remote_index == CB_ERR as isize || remote_index < 0 {
            None
        } else {
            (&(*pointer).remote_keys)
                .get(remote_index as usize)
                .cloned()
                .flatten()
        }
    };
    let (rows, message) = unsafe {
        let state = &mut *pointer;
        state.shown.clear();
        for (index, entry) in state.entries.iter().enumerate() {
            if selected_remote
                .as_ref()
                .is_none_or(|remote| entry.remote_key == *remote)
                && tokens.iter().all(|token| entry.search_key.contains(token))
            {
                state.shown.push(index);
            }
        }
        let rows = state
            .shown
            .iter()
            .map(|index| {
                let entry = &state.entries[*index];
                format!("{}    [{}]", entry.label, entry.remote)
            })
            .collect::<Vec<_>>();
        let message = state.load_error.clone().or_else(|| {
            if state.entries.is_empty() {
                Some("No recent folders found".to_string())
            } else if state.shown.is_empty() {
                Some("No matching folders".to_string())
            } else {
                None
            }
        });
        (rows, message)
    };

    unsafe { SendMessageW(list, WM_SETREDRAW, 0, 0) };
    unsafe { SendMessageW(list, LB_RESETCONTENT, 0, 0) };
    if let Some(message) = message {
        let message = wide(&message);
        unsafe { SendMessageW(list, LB_ADDSTRING, 0, message.as_ptr() as LPARAM) };
    } else {
        for row in rows {
            let row = wide(&row);
            unsafe { SendMessageW(list, LB_ADDSTRING, 0, row.as_ptr() as LPARAM) };
        }
        unsafe { SendMessageW(list, LB_SETCURSEL, 0, 0) };
    }
    unsafe {
        SendMessageW(list, WM_SETREDRAW, 1, 0);
        InvalidateRect(list, std::ptr::null(), 1);
    }
}

unsafe fn populate_remote_filter(window: HWND) {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return;
    }
    let remote_filter = unsafe { (*pointer).remote_filter };
    let mut counts = std::collections::BTreeMap::<String, (String, usize)>::new();
    unsafe {
        for entry in &(*pointer).entries {
            let display = remote_group_display(&entry.remote_key);
            let count = counts
                .entry(entry.remote_key.clone())
                .or_insert((display, 0));
            count.1 += 1;
        }
        SendMessageW(remote_filter, CB_RESETCONTENT, 0, 0);
        let all = wide(&format!("All remotes ({})", (*pointer).entries.len()));
        SendMessageW(remote_filter, CB_ADDSTRING, 0, all.as_ptr() as LPARAM);
        (*pointer).remote_keys.clear();
        (*pointer).remote_keys.push(None);
        for (key, (display, count)) in counts {
            let text = wide(&format!("{display} ({count})"));
            SendMessageW(remote_filter, CB_ADDSTRING, 0, text.as_ptr() as LPARAM);
            (*pointer).remote_keys.push(Some(key));
        }
        SendMessageW(remote_filter, CB_SETCURSEL, 0, 0);
    }
}

unsafe fn launch_selected(window: HWND) {
    let pointer = unsafe { state_ptr(window) };
    if pointer.is_null() {
        return;
    }
    let (list, filter) = unsafe { ((*pointer).list, (*pointer).filter) };
    let selected = unsafe { SendMessageW(list, LB_GETCURSEL, 0, 0) };
    let uri = unsafe {
        let state = &*pointer;
        if selected == LB_ERR as isize || selected < 0 || selected as usize >= state.shown.len() {
            return;
        }
        state.entries[state.shown[selected as usize]].uri.clone()
    };
    let force_new = unsafe { GetKeyState(VK_CONTROL as i32) } < 0;
    let keep_open = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
    match open_folder(window, &uri, force_new) {
        Ok(()) if !keep_open => unsafe {
            DestroyWindow(window);
        },
        Ok(()) => unsafe {
            SetFocus(filter);
        },
        Err(error) => show_error(window, &error),
    }
}

fn open_folder(window: HWND, uri: &str, force_new: bool) -> Result<(), String> {
    let executable = find_code();
    let hidden_window = find_hidden_code_window(uri);
    let arguments = if force_new {
        format!("--new-window --folder-uri \"{uri}\"")
    } else {
        format!("--folder-uri \"{uri}\"")
    };
    let executable = wide(&executable);
    let arguments = wide(&arguments);
    let verb = wide("open");
    let result = unsafe {
        ShellExecuteW(
            window,
            verb.as_ptr(),
            executable.as_ptr(),
            arguments.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err(format!(
            "could not start VS Code (ShellExecute error {})",
            result as isize
        ))
    } else {
        if !hidden_window.is_null() {
            unsafe {
                ShowWindow(hidden_window, SW_RESTORE);
                SetForegroundWindow(hidden_window);
            }
        }
        Ok(())
    }
}

struct WindowSearch {
    needle: String,
    found: HWND,
}

fn find_hidden_code_window(uri: &str) -> HWND {
    let Some(folder_name) = uri
        .split('/')
        .filter(|part| !part.is_empty())
        .next_back()
        .map(percent_decode)
        .filter(|name| !name.is_empty())
    else {
        return std::ptr::null_mut();
    };
    let mut search = WindowSearch {
        needle: folder_name.to_lowercase(),
        found: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(
            Some(find_hidden_window),
            (&mut search as *mut WindowSearch) as LPARAM,
        );
    }
    search.found
}

unsafe extern "system" fn find_hidden_window(window: HWND, lparam: LPARAM) -> i32 {
    if unsafe { IsWindowVisible(window) } != 0 {
        return 1;
    }
    let length = unsafe { GetWindowTextLengthW(window) };
    if length == 0 {
        return 1;
    }
    let mut buffer = vec![0u16; length as usize + 1];
    unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    let title = String::from_utf16_lossy(&buffer[..length as usize]).to_lowercase();
    let search = unsafe { &mut *(lparam as *mut WindowSearch) };
    if title.ends_with(" - visual studio code") && title.contains(&search.needle) {
        search.found = window;
        0
    } else {
        1
    }
}

fn find_code() -> String {
    static RESULT: OnceLock<String> = OnceLock::new();
    RESULT
        .get_or_init(|| {
            let candidates = [
                std::env::var_os("LOCALAPPDATA").map(|root| {
                    PathBuf::from(root)
                        .join("Programs")
                        .join("Microsoft VS Code")
                        .join("Code.exe")
                }),
                std::env::var_os("ProgramFiles").map(|root| {
                    PathBuf::from(root)
                        .join("Microsoft VS Code")
                        .join("Code.exe")
                }),
                std::env::var_os("ProgramFiles(x86)").map(|root| {
                    PathBuf::from(root)
                        .join("Microsoft VS Code")
                        .join("Code.exe")
                }),
            ];
            candidates
                .into_iter()
                .flatten()
                .find(|path| path.is_file())
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "code".to_string())
        })
        .clone()
}

fn control_text(control: HWND) -> String {
    unsafe {
        let length = GetWindowTextLengthW(control);
        let mut buffer = vec![0u16; length as usize + 1];
        GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf16_lossy(&buffer[..length as usize])
    }
}

fn show_error(owner: HWND, message: &str) {
    unsafe {
        MessageBoxW(
            owner,
            wide(message).as_ptr(),
            wide("VS Recent").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn load_entries() -> Result<Vec<Entry>, String> {
    let profile =
        std::env::var_os("USERPROFILE").ok_or_else(|| "USERPROFILE is not set".to_string())?;
    let database = PathBuf::from(profile)
        .join(".vscode-shared")
        .join("sharedStorage")
        .join("state.vscdb");
    if !database.is_file() {
        return Err(format!("database not found at {}", database.display()));
    }
    let json = sqlite::read_recent_json(&database)?
        .ok_or_else(|| "recent-folder history is missing".to_string())?;
    parse_entries(&json)
}

fn parse_entries(json: &str) -> Result<Vec<Entry>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|error| format!("invalid history JSON: {error}"))?;
    let items = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "history JSON does not contain an entries array".to_string())?;
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let Some(uri) = item.get("folderUri").and_then(Value::as_str) else {
            continue;
        };
        if uri.is_empty() {
            continue;
        }
        let label = item
            .get("label")
            .and_then(Value::as_str)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_label(uri));
        let remote = remote_kind(uri);
        let remote_key = remote_key(uri);
        let search_key = format!("{label} {uri} {remote}").to_lowercase();
        entries.push(Entry {
            uri: uri.to_string(),
            label,
            remote,
            remote_key,
            search_key,
        });
    }
    Ok(entries)
}

fn default_label(uri: &str) -> String {
    if let Some(path) = uri.strip_prefix("file:///") {
        percent_decode(path).replace('/', "\\")
    } else {
        uri.to_string()
    }
}

fn remote_kind(uri: &str) -> String {
    if uri
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    {
        return "LOCAL".to_string();
    }
    if let Some(rest) = strip_prefix_ascii_case(uri, "vscode-remote://") {
        let authority = rest.split('/').next().unwrap_or(rest);
        let decoded = percent_decode(authority);
        let mut parts = decoded.splitn(2, '+');
        let kind = parts.next().unwrap_or("");
        let instance = parts.next().unwrap_or("");
        return match kind.to_ascii_lowercase().as_str() {
            "wsl" => format!("WSL: {}", if instance.is_empty() { "?" } else { instance }),
            "ssh-remote" => {
                format!("SSH: {}", if instance.is_empty() { "?" } else { instance })
            }
            "dev-container" => "DEV CONTAINER".to_string(),
            "attached-container" => "CONTAINER".to_string(),
            "codespaces" => "CODESPACE".to_string(),
            "tunnel" => "TUNNEL".to_string(),
            _ => kind.to_ascii_uppercase(),
        };
    }
    if strip_prefix_ascii_case(uri, "vscode-vfs://github").is_some() {
        "GITHUB".to_string()
    } else {
        uri.split(':').next().unwrap_or("?").to_ascii_uppercase()
    }
}

fn remote_key(uri: &str) -> String {
    if uri
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
    {
        return "local".to_string();
    }
    if let Some(rest) = strip_prefix_ascii_case(uri, "vscode-remote://") {
        let authority = rest.split('/').next().unwrap_or(rest);
        let decoded = percent_decode(authority);
        let kind = decoded.split('+').next().unwrap_or("").to_ascii_lowercase();
        return match kind.as_str() {
            "ssh-remote" => "ssh".to_string(),
            "attached-container" => "container".to_string(),
            "codespaces" => "codespace".to_string(),
            _ => kind,
        };
    }
    if strip_prefix_ascii_case(uri, "vscode-vfs://github").is_some() {
        "github".to_string()
    } else if strip_prefix_ascii_case(uri, "vscode-vfs://").is_some() {
        "vfs".to_string()
    } else {
        uri.split(':')
            .next()
            .filter(|scheme| !scheme.is_empty())
            .unwrap_or("unknown")
            .to_ascii_lowercase()
    }
}

fn remote_group_display(key: &str) -> String {
    match key {
        "local" => "Local".to_string(),
        "wsl" => "WSL".to_string(),
        "ssh" => "SSH".to_string(),
        "dev-container" => "Dev Container".to_string(),
        "container" => "Container".to_string(),
        "codespace" => "Codespace".to_string(),
        "tunnel" => "Tunnel".to_string(),
        "github" => "GitHub".to_string(),
        "vfs" => "VFS".to_string(),
        "unknown" => "Unknown".to_string(),
        _ => key.to_ascii_uppercase(),
    }
}

fn strip_prefix_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                decoded.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn demo_entries() -> Vec<Entry> {
    parse_entries(
        r#"{"entries":[
          {"label":"vsrecent-rust","folderUri":"file:///c%3A/Users/caleb/apps/vsrecent-rust"},
          {"label":"dotfiles","folderUri":"vscode-remote://wsl%2BUbuntu/home/caleb/dotfiles"},
          {"label":"training","folderUri":"vscode-remote://ssh-remote%2Bgpu-rig/data/training"},
          {"label":"api-service","folderUri":"vscode-remote://dev-container%2Babc/workspaces/api"}
        ]}"#,
    )
    .expect("demo JSON is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_folders_and_skips_files() {
        let entries = parse_entries(
            r#"{"entries":[
                {"folderUri":"file:///c%3A/code/project"},
                {"fileUri":"file:///c%3A/code/readme.md"},
                {"label":"remote","folderUri":"vscode-remote://wsl%2BUbuntu/home/me/app"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "c:\\code\\project");
        assert_eq!(entries[0].remote_key, "local");
        assert_eq!(entries[1].remote, "WSL: Ubuntu");
        assert_eq!(entries[1].remote_key, "wsl");
        assert!(entries[1].search_key.contains("wsl"));
    }

    #[test]
    fn decodes_utf8_uri_components() {
        assert_eq!(percent_decode("caf%C3%A9%20app"), "café app");
    }

    #[test]
    fn assigns_distinct_remote_colors() {
        assert_ne!(row_colors("LOCAL", false), row_colors("WSL: Ubuntu", false));
        assert_ne!(
            row_colors("WSL: Ubuntu", false),
            row_colors("SSH: host", false)
        );
        assert_ne!(
            row_colors("SSH: host", false),
            row_colors("DEV CONTAINER", false)
        );
        assert_ne!(row_colors("LOCAL", false), row_colors("LOCAL", true));
    }
}
