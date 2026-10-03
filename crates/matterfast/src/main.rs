//! The desktop executable; the application itself is the library.

// Without this a release build on Windows opens a console window behind the
// real one. Debug builds keep the console: it is where the log goes.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    // Android never runs this: the activity loads the library instead.
    #[cfg(not(target_os = "android"))]
    matterfast::run();
}
