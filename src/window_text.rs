//! Replacements for the functions that put Shift-JIS text into windows:
//! message boxes, dialog items and window titles.

use std::ffi::c_void;
use std::ptr::null;

use windows_sys::Win32::Foundation::{HMODULE, HWND};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcW, GWL_STYLE, GetWindowLongW, HMENU, MessageBoxW,
    SetDlgItemTextW, SetWindowTextA, WM_SETTEXT, WS_CAPTION,
};
use windows_sys::core::BOOL;

use crate::hooks::{japanese_to_wide, keeping_last_error};

pub(crate) extern "system" fn message_box(
    window: HWND,
    text: *const u8,
    caption: *const u8,
    style: u32,
) -> i32 {
    let text = japanese_to_wide(text);
    let caption = (!caption.is_null()).then(|| japanese_to_wide(caption));
    let caption_pointer = caption.as_ref().map_or(null(), |caption| caption.as_ptr());
    unsafe { MessageBoxW(window, text.as_ptr(), caption_pointer, style) }
}

pub(crate) extern "system" fn set_dialog_item_text(
    dialog: HWND,
    item: i32,
    text: *const u8,
) -> BOOL {
    unsafe { SetDlgItemTextW(dialog, item, japanese_to_wide(text).as_ptr()) }
}

/// Old programs create ANSI windows, so a title set through their own
/// window procedure gets converted with the Western code page and garbled.
/// Handing the converted title straight to DefWindowProcW skips that step.
fn set_title(window: HWND, title: *const u8) -> bool {
    if window.is_null() || title.is_null() {
        return false;
    }

    let style = unsafe { GetWindowLongW(window, GWL_STYLE) } as u32;

    if style & WS_CAPTION != WS_CAPTION {
        return false;
    }

    let title = japanese_to_wide(title);
    unsafe { DefWindowProcW(window, WM_SETTEXT, 0, title.as_ptr() as isize) };
    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) extern "system" fn create_window(
    extended_style: u32,
    class: *const u8,
    title: *const u8,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    menu: HMENU,
    instance: HMODULE,
    parameter: *const c_void,
) -> HWND {
    let window = unsafe {
        CreateWindowExA(
            extended_style,
            class,
            title,
            style,
            x,
            y,
            width,
            height,
            parent,
            menu,
            instance,
            parameter,
        )
    };

    if !window.is_null() {
        keeping_last_error(|| _ = set_title(window, title));
    }

    window
}

pub(crate) extern "system" fn set_window_text(window: HWND, text: *const u8) -> BOOL {
    if set_title(window, text) {
        return 1;
    }

    unsafe { SetWindowTextA(window, text) }
}
