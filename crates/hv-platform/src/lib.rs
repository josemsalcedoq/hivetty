//! OS-specific integrations. The only crate allowed to use unsafe code.

// Platform APIs (Win32, Cocoa, D-Bus) need FFI; unsafe is confined to this crate.
#![allow(unsafe_code)]
