//! A bridge library between game server and VC server.

#![feature(macro_attr)]

mod byond;

#[byond::function]
fn get_version() -> &str { env!("CARGO_PKG_VERSION") }

#[byond::function]
fn echo(arg: String) -> &str { arg.as_str() }

#[byond::function]
fn void() {}

// /// Catches panics and converts them into error codes for FFI functions.
// pub fn ffi_guard<F>(f: F) -> i32
// where
//     F: FnOnce() -> Result<(), i32>,
// {
//     match panic::catch_unwind(AssertUnwindSafe(f)) {
//         Ok(Ok(())) => 0,
//         Ok(Err(err)) => err,
//         Err(_) => -1,
//     }
// }
