//! Makes every export of the real version.dll a forwarder: the program asks
//! this DLL for GetFileVersionInfoA and Windows hands it the real one.
//!
//! Forwarding by plain name ("version.GetFileVersionInfoA") would resolve to
//! this DLL again, so the target is the full path to the system copy, written
//! through \\.\GLOBALROOT\SystemRoot so it resolves to whichever drive Windows
//! is on. SysWOW64 holds the 32-bit system DLLs (matching the 32-bit build)
//! and only exists on 64-bit Windows, so that's what the proxy needs.

const EXPORTS: [&str; 17] = [
    "GetFileVersionInfoA",
    "GetFileVersionInfoByHandle",
    "GetFileVersionInfoExA",
    "GetFileVersionInfoExW",
    "GetFileVersionInfoSizeA",
    "GetFileVersionInfoSizeExA",
    "GetFileVersionInfoSizeExW",
    "GetFileVersionInfoSizeW",
    "GetFileVersionInfoW",
    "VerFindFileA",
    "VerFindFileW",
    "VerInstallFileA",
    "VerInstallFileW",
    "VerLanguageNameA",
    "VerLanguageNameW",
    "VerQueryValueA",
    "VerQueryValueW",
];

const SYSTEM_COPY: &str = r"\\.\GLOBALROOT\SystemRoot\SysWOW64\version";

fn main() {
    for name in EXPORTS {
        println!("cargo::rustc-cdylib-link-arg=/EXPORT:{name}={SYSTEM_COPY}.{name}");
    }
}
