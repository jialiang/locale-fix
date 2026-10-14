//! Replacements for Windows functions that programs call with Shift-JIS text:
//! character handling, conversion, fonts and DLL loading. The ones that put
//! text into windows are in window_text.rs.

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{GetLastError, HANDLE, HMODULE, SetLastError};
use windows_sys::Win32::Globalization::{IsDBCSLeadByteEx, MultiByteToWideChar};
use windows_sys::Win32::Graphics::Gdi::{CreateFontIndirectW, HFONT, LOGFONTA, LOGFONTW};
use windows_sys::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_FLAGS, LoadLibraryA, LoadLibraryExA, LoadLibraryExW, LoadLibraryW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{CharNextExA, CharPrevExA};
use windows_sys::core::BOOL;

use crate::patch_program_modules;

const JAPANESE: u32 = 932;
const JAPANESE_LOCALE: u32 = 0x0411;

// MultiByteToWideChar's "whatever the system uses" code pages. On a Japanese
// system all three (Windows, console and per-thread) are 932.
const SYSTEM_ANSI: u32 = 0;
const SYSTEM_OEM: u32 = 1;
const THREAD_ANSI: u32 = 3;

/// Runs extra work after a real call without changing the error code the
/// caller may read next (GetLastError).
pub(crate) fn keeping_last_error(work: impl FnOnce()) {
    let error = unsafe { GetLastError() };
    work();
    unsafe { SetLastError(error) };
}

/// Converts a null-terminated Shift-JIS string to null-terminated UTF-16.
pub(crate) fn japanese_to_wide(text: *const u8) -> Vec<u16> {
    if text.is_null() {
        return vec![0];
    }

    let length = unsafe { MultiByteToWideChar(JAPANESE, 0, text, -1, null_mut(), 0) };
    let mut wide = vec![0u16; length.max(1) as usize];
    unsafe { MultiByteToWideChar(JAPANESE, 0, text, -1, wide.as_mut_ptr(), length) };
    wide
}

pub(crate) extern "system" fn is_lead_byte(byte: u8) -> BOOL {
    unsafe { IsDBCSLeadByteEx(JAPANESE, byte) }
}

pub(crate) extern "system" fn to_wide(
    code_page: u32,
    flags: u32,
    source: *const u8,
    source_length: i32,
    target: *mut u16,
    target_length: i32,
) -> i32 {
    let is_system_code_page = matches!(code_page, SYSTEM_ANSI | SYSTEM_OEM | THREAD_ANSI);
    let code_page = if is_system_code_page { JAPANESE } else { code_page };
    unsafe { MultiByteToWideChar(code_page, flags, source, source_length, target, target_length) }
}

pub(crate) extern "system" fn system_locale() -> u32 {
    JAPANESE_LOCALE
}

pub(crate) extern "system" fn next_character(current: *const u8) -> *mut u8 {
    unsafe { CharNextExA(JAPANESE as u16, current, 0) }
}

pub(crate) extern "system" fn previous_character(start: *const u8, current: *const u8) -> *mut u8 {
    unsafe { CharPrevExA(JAPANESE as u16, start, current, 0) }
}

/// Patches a DLL the program just loaded, unless the load failed.
fn patch_after_load(module: HMODULE) -> HMODULE {
    if !module.is_null() {
        keeping_last_error(patch_program_modules);
    }

    module
}

pub(crate) extern "system" fn load_library(name: *const u8) -> HMODULE {
    patch_after_load(unsafe { LoadLibraryA(name) })
}

pub(crate) extern "system" fn load_library_wide(name: *const u16) -> HMODULE {
    patch_after_load(unsafe { LoadLibraryW(name) })
}

pub(crate) extern "system" fn load_library_ex(
    name: *const u8,
    file: HANDLE,
    flags: u32,
) -> HMODULE {
    patch_after_load(unsafe { LoadLibraryExA(name, file, flags as LOAD_LIBRARY_FLAGS) })
}

pub(crate) extern "system" fn load_library_ex_wide(
    name: *const u16,
    file: HANDLE,
    flags: u32,
) -> HMODULE {
    patch_after_load(unsafe { LoadLibraryExW(name, file, flags as LOAD_LIBRARY_FLAGS) })
}

/// Font names like ＭＳ Ｐゴシック arrive in Shift-JIS, so convert them with
/// code page 932 rather than letting Windows misread them as Western text.
pub(crate) extern "system" fn create_font(font: *const LOGFONTA) -> HFONT {
    // The real CreateFontIndirectA returns no font for a null description.
    let Some(font) = (unsafe { font.as_ref() }) else { return null_mut() };

    let mut wide: LOGFONTW = unsafe { std::mem::zeroed() };
    wide.lfHeight = font.lfHeight;
    wide.lfWidth = font.lfWidth;
    wide.lfEscapement = font.lfEscapement;
    wide.lfOrientation = font.lfOrientation;
    wide.lfWeight = font.lfWeight;
    wide.lfItalic = font.lfItalic;
    wide.lfUnderline = font.lfUnderline;
    wide.lfStrikeOut = font.lfStrikeOut;
    wide.lfCharSet = font.lfCharSet;
    wide.lfOutPrecision = font.lfOutPrecision;
    wide.lfClipPrecision = font.lfClipPrecision;
    wide.lfQuality = font.lfQuality;
    wide.lfPitchAndFamily = font.lfPitchAndFamily;

    let name = font.lfFaceName.as_ptr().cast();
    let capacity = wide.lfFaceName.len() as i32 - 1;
    let target = wide.lfFaceName.as_mut_ptr();
    unsafe { MultiByteToWideChar(JAPANESE, 0, name, -1, target, capacity) };
    unsafe { CreateFontIndirectW(&wide) }
}
