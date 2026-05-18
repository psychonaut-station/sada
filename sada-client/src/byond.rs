//! This module provides utilities for creating BYOND-compatible functions in Rust.

use std::{
    backtrace::Backtrace,
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_int},
    fs::OpenOptions,
    io::Write,
    panic,
    sync::Once,
};

thread_local! {
    /// todo
    #[doc(hidden)]
    pub static LAST_RETURN: RefCell<CString> = Default::default();
}

/// todo
#[doc(hidden)]
pub static VOID_RETURN: c_char = 0;

/// wip
///
/// # Safety
///
/// wip
#[doc(hidden)]
pub unsafe fn __parse_args<'a, const N: usize>(argc: c_int, argv: *const *const c_char) -> [&'a str; N] {
    let mut args = [""; N];
    for (i, arg) in args.iter_mut().enumerate() {
        if i >= argc as usize {
            break;
        }
        let c_str = unsafe { CStr::from_ptr(*argv.add(i)) };
        *arg = c_str.to_str().unwrap_or_default();
    }
    args
}

#[doc(hidden)]
pub fn __set_panic_hook() {
    static PANIC_HOOK: Once = Once::new();
    PANIC_HOOK.call_once(|| {
        let hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            hook(info);

            let mut file = match OpenOptions::new().append(true).create(true).open("sada_panic.log") {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("failed to open panic log file: {e:?}");
                    return;
                },
            };
            let payload = info.payload_as_str().unwrap_or("<non-string panic payload>");

            if let Err(e) = file.write_all(payload.as_bytes()) {
                eprintln!("failed to write error payload: {e:?}");
            }
            let _ = file.write_all(b"\n");

            if let Err(e) = file.write_all(Backtrace::capture().to_string().as_bytes()) {
                eprintln!("failed to write backtrace: {e:?}");
            }
            let _ = file.write_all(b"\n");
        }));
    });
}

/// todo
#[doc(hidden)]
macro_rules! __byond_return {
    ($res:ident ->) => {
        &$crate::byond::VOID_RETURN
    };
    ($res:ident -> $ret:ty) => {{
        $crate::byond::LAST_RETURN.with(|last| {
            last.replace(::std::ffi::CString::new(<$ret as ToString>::to_string(&$res)).unwrap_or_default());
            last.borrow().as_ptr()
        })
    }};
}

/// Returns the number of arguments passed to the macro.
#[doc(hidden)]
macro_rules! __count_args {
    () => { 0 };
    ($head:ident $(, $tail:ident)*) => { 1 + $crate::byond::__count_args!($($tail),*) };
}

/// wodo
macro_rules! __function {
    (@panic_hook) => {
        $crate::byond::__set_panic_hook();
    };
    (@panic_hook $skip_hook:literal) => {
        if !$skip_hook { $crate::byond::__set_panic_hook(); }
    };

    attr($(skip_hook = $skip_hook:literal)?) (fn $name:ident() $(-> $ret:ty)? $body:block) => {
        #[unsafe(no_mangle)]
        #[allow(missing_docs, clippy::missing_safety_doc)]
        pub unsafe extern "C" fn $name(
            _argc: ::std::ffi::c_int, _argv: *const *const ::std::ffi::c_char
        ) -> *const ::std::ffi::c_char {
            $crate::byond::function!(@panic_hook $($skip_hook)?);
            let __result = $body;
            $crate::byond::__byond_return!(__result -> $($ret)?)
        }
    };

    attr($(skip_hook = $skip_hook:literal)?) (fn $name:ident($($arg:ident : $arg_ty:ty),*) $(-> $ret:ty)? $body:block) => {
        #[unsafe(no_mangle)]
        #[allow(missing_docs, clippy::missing_safety_doc)]
        pub unsafe extern "C" fn $name(
            _argc: ::std::ffi::c_int, _argv: *const *const ::std::ffi::c_char
        ) -> *const ::std::ffi::c_char {
            $crate::byond::function!(@panic_hook $($skip_hook)?);
            let __args = unsafe { $crate::byond::__parse_args::<{ $crate::byond::__count_args!($($arg),*) }>(_argc, _argv) };
            let mut __argi = 0;
            $(let $arg: $arg_ty = ::std::str::FromStr::from_str(__args[__argi]).unwrap_or_default(); __argi += 1;)*
            let __result = $body;
            $crate::byond::__byond_return!(__result -> $($ret)?)
        }
    };
}

pub(crate) use __byond_return;
pub(crate) use __count_args;
#[doc(inline)]
pub(crate) use __function as function;
