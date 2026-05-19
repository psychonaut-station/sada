#![feature(macro_attr)]

//! A bridge library between game server and VC server.

mod byond;
mod control;

use byond::byondapi;
use meowtonin::ByondValue;
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

/// todo
///
/// # Safety
///
/// todo
#[unsafe(no_mangle)]
pub unsafe extern "C" fn echo2(_argc: u32, _argv: *mut ByondValue) -> ByondValue {
    ByondValue::new_string("byondapi works!")
}

#[byondapi]
fn echo3(arg: String) -> String { arg }

#[byondapi]
fn panicing2() -> i32 {
    panic!("This function panics too!");
    #[allow(unreachable_code)]
    1
}

#[byondapi]
fn update_position(mob: ByondValue, x: i32, y: i32) {
    let Ok(name) = mob.read_var::<_, String>("name") else {
        eprintln!("Failed to read mob name");
        return;
    };

    println!("Updating position of {} to ({}, {})", name, x, y);
}
