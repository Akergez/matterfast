# Vendored crates

Copies of crates from crates.io that had to be changed, wired in through
`[patch.crates-io]` in the workspace `Cargo.toml`. Each one is the published
source with the smallest patch that does the job; drop it when upstream no
longer needs it.

## gpui-pre-windows 0.3.7

GPUI's Windows backend, Apache-2.0 (`LICENSE-APACHE` inside). Only `build.rs`
differs from the published crate.

Upstream compiles its HLSL shaders with `fxc.exe` behind
`#[cfg(target_os = "windows")]`, which in a build script means the *host*.
Cross-compiling from Linux, the script therefore did nothing and the release
build failed on the missing `shaders_bytes.rs`; a debug build compiles the
shaders at run time from a source path on the build machine, so it is not
something to ship either. The patch asks about the *target* instead, and on a
non-Windows host runs `fxc.exe` through wine. `build-aux/build-windows.sh`
fetches `fxc.exe` and sets `GPUI_FXC_PATH`.

Nothing here is compiled for Linux or macOS.
