#![feature(macro_attr, never_type)]

//! A bridge library between game server and VC server.

mod byond;

#[byond::function]
fn get_version() -> &str { env!("CARGO_PKG_VERSION") }

#[byond::function]
fn echo(arg: String) -> &str { arg.as_str() }

#[byond::function]
fn void() {}

#[byond::function]
fn panicing() -> i32 {
    panic!("This function panics!");
    #[allow(unreachable_code)]
    1
}
