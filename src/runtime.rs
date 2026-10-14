//! Switches the C runtime's multibyte functions (_mbsstr, _ismbblead and
//! friends, which parsers in older programs often use) to Japanese.
//!
//! The switch happens when the program calls __getmainargs to read its command
//! line: after every DLL (the C runtime included) has initialised and before
//! any of the program's own code runs.

use std::ffi::{CStr, c_void};
use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

const JAPANESE: i32 = 932;

/// The shared C runtime DLLs of Visual C++ 6 through 2013. Programs built with
/// Visual Studio 2015 or later (the Universal C runtime) start up without
/// __getmainargs, so they aren't covered.
pub(crate) const RUNTIMES: [&CStr; 8] = [
    c"msvcrt.dll",
    c"msvcr70.dll",
    c"msvcr71.dll",
    c"msvcr80.dll",
    c"msvcr90.dll",
    c"msvcr100.dll",
    c"msvcr110.dll",
    c"msvcr120.dll",
];

/// Each runtime's real __getmainargs, recorded when the program's imports are
/// patched, for the hook below to call through to.
pub(crate) static ORIGINALS: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];

/// One hook per runtime, so each knows which runtime to switch and call.
pub(crate) const HOOKS: [*const (); 8] = [
    read_main_arguments::<0> as *const (),
    read_main_arguments::<1> as *const (),
    read_main_arguments::<2> as *const (),
    read_main_arguments::<3> as *const (),
    read_main_arguments::<4> as *const (),
    read_main_arguments::<5> as *const (),
    read_main_arguments::<6> as *const (),
    read_main_arguments::<7> as *const (),
];

type MainArguments =
    unsafe extern "C" fn(*mut i32, *mut c_void, *mut c_void, i32, *mut c_void) -> i32;

extern "C" fn read_main_arguments<const RUNTIME: usize>(
    count: *mut i32,
    arguments: *mut c_void,
    environment: *mut c_void,
    is_wildcard_expanded: i32,
    startup: *mut c_void,
) -> i32 {
    set_code_page(RUNTIMES[RUNTIME]);

    // The original is recorded before the import is redirected, so it's set.
    let original = ORIGINALS[RUNTIME].load(Ordering::Acquire);
    let original: MainArguments = unsafe { std::mem::transmute(original) };
    unsafe { original(count, arguments, environment, is_wildcard_expanded, startup) }
}

fn set_code_page(runtime: &CStr) {
    let module = unsafe { GetModuleHandleA(runtime.as_ptr().cast()) };

    if module.is_null() {
        return;
    }

    let Some(set_code_page) = (unsafe { GetProcAddress(module, c"_setmbcp".as_ptr().cast()) })
    else {
        return;
    };

    let set_code_page: unsafe extern "C" fn(i32) -> i32 =
        unsafe { std::mem::transmute(set_code_page) };
    unsafe { set_code_page(JAPANESE) };
}
