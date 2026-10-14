# locale-fix

A `version.dll` proxy that runs old Japanese (Shift-JIS) Windows programs
correctly on a non-Japanese system locale, without changing the system locale
or using a general-purpose locale emulator.

## Usage

Copy `version.dll` next to the program's exe and start the exe as usual.
Delete the file to undo.

Windows looks for DLLs in the program's own folder before `System32`, so the
program loads this copy instead of the real `version.dll`. Every function it
exports forwards straight to the real one. The fix runs as it loads.

This only works for programs that import `version.dll`. Check with any PE
viewer.

## What it does

It switches the C runtime to code page 932 and redirects a small, explicit
list of Windows text functions, in modules loaded from the program's own
folder only. System DLLs are never patched.

| Function                                       | Fix                                                                               |
| ---------------------------------------------- | --------------------------------------------------------------------------------- |
| `__getmainargs` (C runtime startup)            | Calls `_setmbcp(932)` first, so `_mbsstr`, `_ismbblead` and friends use Shift-JIS |
| `IsDBCSLeadByte`, `CharNextA`, `CharPrevA`     | Code page 932 versions                                                            |
| `MultiByteToWideChar`                          | System code pages swapped for 932                                                 |
| `GetSystemDefaultLCID`                         | Reports Japanese (0x0411)                                                         |
| `CreateFontIndirectA`                          | Face name converted from Shift-JIS                                                |
| `MessageBoxA`, `SetDlgItemTextA`               | Text converted, Unicode version called                                            |
| `CreateWindowExA`, `SetWindowTextA`            | Window titles converted                                                           |
| `LoadLibraryA`, `LoadLibraryW` and `Ex` forms  | Patches DLLs the program loads later                                              |

The C runtime fix covers the shared runtime DLLs of Visual C++ 6 through 2013
(`msvcrt.dll` and `msvcr70.dll` to `msvcr120.dll`). The proxy loads while
Windows is still starting the program, possibly before the C runtime has
initialised. So it sets the runtime's code page later, when the program reads
its command line, the first moment the runtime is certain to be ready.

## Antivirus warnings

Placing a DLL so a program loads it instead of a system one (DLL sideloading)
is also a malware technique, so antivirus tools may flag the unsigned
`version.dll`, now or after a future signature update. It reads and writes
nothing outside the program it loads into. The source is short enough to
audit. Building it yourself is the safest way to use it.

## Known gaps

1. 32-bit programs only, on 64-bit Windows: the forwarders point at the
   32-bit system copy in `SysWOW64`, which 32-bit Windows doesn't have.
2. Japanese only: the code page is fixed at 932.
3. Not yet covered: file APIs with Japanese paths (`CreateFileA`,
   `FindFirstFileA`), menus (`AppendMenuA`, `InsertMenuItemA`), `GetACP`,
   `WideCharToMultiByte`, `GetWindowTextA` and resource strings.
4. Functions a program looks up at runtime with `GetProcAddress` aren't
   redirected. Neither are DLLs loaded by delay-loading or by system DLLs.
5. Programs with a statically linked C runtime, or built with Visual Studio
   2015 or later (the Universal C runtime), don't get the `_setmbcp` fix.
6. Controls in the program's own dialogs (edit boxes, labels) still show
   question marks for Japanese text. Windows converts their text inside
   `user32` with the system code page, which no import redirect reaches.

## Building

Needs:

1. The `i686-pc-windows-msvc` Rust target
   (`rustup target add i686-pc-windows-msvc`).
2. The MSVC Build Tools (or Visual Studio) with the x86 libraries, plus the
   Windows SDK. Linking the bundled C runtime needs their libraries
   (`libcmt.lib`, `kernel32.lib` and so on), even though `rust-lld` does the
   linking.

Build from this folder so `.cargo/config.toml` applies:

```
cargo build --release
```

The output is `target/i686-pc-windows-msvc/release/version.dll`. Linking uses
`rust-lld`, which ships with Rust. MSVC's `link.exe` (14.51) rejects the
export forwarders the proxy is built from.

Tested with 白銀のソレイユ -Successor of Wyrd- (ExHIBIT engine, 2007).
