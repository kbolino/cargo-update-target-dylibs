# cargo-update-target-dylibs

A tool for the Rust `cargo` build process which copies any dynamic libraries
(`.dll` on Windows, `.dylib` on macOS, `.so` on Linux/BSD/etc.) that were used
to build dependent crates into the target directory of the calling crate.
