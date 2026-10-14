//! A version.dll proxy that makes a Japanese program treat text as Shift-JIS
//! (code page 932) on a Western system.
//!
//! Placed next to the program's exe, it loads instead of the real version.dll
//! (Windows searches the program's folder before System32) and forwards every
//! export to the real one (see build.rs).
//!
//! Older Japanese programs store text in Shift-JIS (where one character can
//! span two bytes) and lean on the system code page to tell where characters
//! start. Rather than faking the locale for the whole process, this switches
//! the C runtime to Japanese (see runtime.rs) and redirects a few Windows text
//! functions (see hooks.rs and window_text.rs) to versions with the code page
//! fixed at 932. Only modules loaded from the program's own folder are
//! redirected, never system DLLs.

mod hooks;
mod runtime;
mod window_text;

use std::ffi::{CStr, c_void};
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_IMPORT, IMAGE_NT_HEADERS32,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleA, GetProcAddress,
};
use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
use windows_sys::Win32::System::ProcessStatus::K32EnumProcessModules;
use windows_sys::Win32::System::SystemServices::{
    DLL_PROCESS_ATTACH, IMAGE_DOS_HEADER, IMAGE_IMPORT_DESCRIPTOR,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::core::BOOL;

struct Paths {
    program_folder: String,
    own_module: String,
}

static PATHS: OnceLock<Paths> = OnceLock::new();

/// Keeps one thread from restoring a page's protection while another is still
/// writing to it. Nothing that takes the Windows loader lock runs under it, so
/// a thread starting a DLL (which holds the loader lock) can't deadlock on it.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Each row names an imported function (its library and name) and the
/// replacement its import table entries are redirected to. The C runtime rows
/// come from runtime.rs.
const TEXT_REPLACEMENTS: [(&CStr, &CStr, *const ()); 14] = [
    (c"gdi32.dll", c"CreateFontIndirectA", hooks::create_font as *const ()),
    (c"kernel32.dll", c"IsDBCSLeadByte", hooks::is_lead_byte as *const ()),
    (c"kernel32.dll", c"MultiByteToWideChar", hooks::to_wide as *const ()),
    (c"kernel32.dll", c"GetSystemDefaultLCID", hooks::system_locale as *const ()),
    (c"kernel32.dll", c"LoadLibraryA", hooks::load_library as *const ()),
    (c"kernel32.dll", c"LoadLibraryW", hooks::load_library_wide as *const ()),
    (c"kernel32.dll", c"LoadLibraryExA", hooks::load_library_ex as *const ()),
    (c"kernel32.dll", c"LoadLibraryExW", hooks::load_library_ex_wide as *const ()),
    (c"user32.dll", c"CharNextA", hooks::next_character as *const ()),
    (c"user32.dll", c"CharPrevA", hooks::previous_character as *const ()),
    (c"user32.dll", c"MessageBoxA", window_text::message_box as *const ()),
    (c"user32.dll", c"SetDlgItemTextA", window_text::set_dialog_item_text as *const ()),
    (c"user32.dll", c"CreateWindowExA", window_text::create_window as *const ()),
    (c"user32.dll", c"SetWindowTextA", window_text::set_window_text as *const ()),
];

/// Runs while Windows is still starting the program, possibly before the C
/// runtime has initialised. So this only patches imports: the runtime's code
/// page is set later (see runtime.rs).
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(own_module: HMODULE, reason: u32, _: *mut c_void) -> BOOL {
    if reason != DLL_PROCESS_ATTACH {
        return 1;
    }

    // Keep the trailing backslash so the prefix check in patch_program_modules
    // can't match a sibling folder. Without a folder, patch nothing.
    let program_path = module_path(unsafe { GetModuleHandleA(null_mut()) });
    let Some(separator) = program_path.rfind('\\') else { return 1 };

    let program_folder = program_path[..=separator].to_lowercase();
    let own_module = module_path(own_module).to_lowercase();
    PATHS.get_or_init(|| Paths { program_folder, own_module });

    patch_program_modules();
    1
}

fn module_path(module: HMODULE) -> String {
    let mut buffer = [0u16; 1024];
    let length = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) };
    String::from_utf16_lossy(&buffer[..length as usize])
}

/// Pairs each loaded original function's address with its replacement,
/// recording the originals that runtime.rs hooks call through to.
fn targets() -> Vec<(usize, usize)> {
    let text_rows = TEXT_REPLACEMENTS
        .iter()
        .map(|&(library, function, hook)| (library, function, hook, None::<&AtomicUsize>));
    let runtime_rows = (0..runtime::RUNTIMES.len()).map(|index| {
        let original = Some(&runtime::ORIGINALS[index]);
        (runtime::RUNTIMES[index], c"__getmainargs", runtime::HOOKS[index], original)
    });
    let mut targets = Vec::new();

    for (library, function, hook, original_slot) in text_rows.chain(runtime_rows) {
        // A null handle would make GetProcAddress search the exe instead.
        let library = unsafe { GetModuleHandleA(library.as_ptr().cast()) };

        if library.is_null() {
            continue;
        }

        let Some(original) = (unsafe { GetProcAddress(library, function.as_ptr().cast()) }) else {
            continue;
        };

        if let Some(slot) = original_slot {
            slot.store(original as usize, Ordering::Release);
        }

        targets.push((original as usize, hook as usize));
    }

    targets
}

/// Patches every module loaded from the program folder (programs often load
/// DLLs late, so the LoadLibrary replacements call this again after each load).
pub(crate) fn patch_program_modules() {
    let Some(paths) = PATHS.get() else { return };
    let targets = targets();

    let mut modules = [null_mut(); 512];
    let mut needed = 0;
    let size = size_of::<[HMODULE; 512]>() as u32;
    unsafe { K32EnumProcessModules(GetCurrentProcess(), modules.as_mut_ptr(), size, &mut needed) };
    let count = (needed as usize / size_of::<HMODULE>()).min(modules.len());

    for &module in &modules[..count] {
        let path = module_path(module).to_lowercase();

        if !path.starts_with(&paths.program_folder) || path == paths.own_module {
            continue;
        }

        unsafe { patch_imports(module as usize, &targets) };
    }
}

/// Swaps import table entries that point at an original function.
unsafe fn patch_imports(base: usize, targets: &[(usize, usize)]) {
    let dos_header = unsafe { &*(base as *const IMAGE_DOS_HEADER) };
    let headers = (base + dos_header.e_lfanew as usize) as *const IMAGE_NT_HEADERS32;
    let directories = unsafe { &(*headers).OptionalHeader.DataDirectory };
    let imports = directories[IMAGE_DIRECTORY_ENTRY_IMPORT as usize].VirtualAddress as usize;

    if imports == 0 {
        return;
    }

    // One descriptor per imported DLL, ending with an all-zero one.
    let mut descriptor = (base + imports) as *const IMAGE_IMPORT_DESCRIPTOR;

    while unsafe { (*descriptor).Name } != 0 {
        let mut entry = (base + unsafe { (*descriptor).FirstThunk } as usize) as *mut usize;

        while unsafe { *entry } != 0 {
            let current = unsafe { *entry };
            let target = targets.iter().find(|(original, _)| *original == current);

            if let Some(&(_, replacement)) = target {
                unsafe { write_entry(entry, replacement) };
            }

            entry = unsafe { entry.add(1) };
        }

        descriptor = unsafe { descriptor.add(1) };
    }
}

/// Import tables sit in read-only memory, so each write briefly unlocks the
/// entry. Execute stays allowed in case the table shares a page with code.
unsafe fn write_entry(entry: *mut usize, replacement: usize) {
    let _guard = WRITE_LOCK.lock();
    let size = size_of::<usize>();
    let mut old = 0;

    if unsafe { VirtualProtect(entry.cast(), size, PAGE_EXECUTE_READWRITE, &mut old) } == 0 {
        return;
    }

    unsafe { *entry = replacement };
    unsafe { VirtualProtect(entry.cast(), size, old, &mut old) };
}
