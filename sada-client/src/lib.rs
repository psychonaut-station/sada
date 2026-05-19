#![feature(macro_attr)]

//! A bridge library between game server and VC server.

mod byond;
mod control;

use sada_common::ControlResponse;

/// Encodes a [`ControlResponse`] into a JSON string. If encoding fails, returns an error message as JSON string.
fn encode_response(response: ControlResponse) -> String {
    serde_json::to_string(&response).unwrap_or_else(|err| {
        serde_json::json!({
            "error": {
                "message": format!("failed to encode response: {err}"),
            },
        })
        .to_string()
    })
}

#[byond::function]
fn get_version() -> &str { env!("CARGO_PKG_VERSION") }

#[byond::function]
fn init(path: &str) -> String { encode_response(control::init(path)) }

#[byond::function]
fn set_ptt(ckey: &str, pressed: &str) -> String { encode_response(control::set_ptt(ckey.to_owned(), pressed == "1")) }

#[byond::function]
fn echo(arg: &str) -> &str { arg }

#[byond::function]
fn void() {}

#[byond::function]
fn panicing() -> i32 {
    panic!("This function panics!");
    #[allow(unreachable_code)]
    1
}
