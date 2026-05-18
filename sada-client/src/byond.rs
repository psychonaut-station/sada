//! This module provides utilities for creating BYOND-compatible functions in Rust.

use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_int},
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
    attr() ($($token:tt)*) => {
        $crate::byond::function!($($token)*);
    };
    (fn $name:ident() $(-> $ret:ty)? $body:block) => {
        #[unsafe(no_mangle)]
        #[allow(missing_docs, clippy::missing_safety_doc)]
        pub unsafe extern "C" fn $name(
            _argc: ::std::ffi::c_int, _argv: *const *const ::std::ffi::c_char
        ) -> *const ::std::ffi::c_char {
            let __result = $body;
            $crate::byond::__byond_return!(__result -> $($ret)?)
        }
    };
    (fn $name:ident($($arg:ident : $arg_ty:ty),*) $(-> $ret:ty)? $body:block) => {
        #[unsafe(no_mangle)]
        #[allow(missing_docs, clippy::missing_safety_doc)]
        pub unsafe extern "C" fn $name(
            _argc: ::std::ffi::c_int, _argv: *const *const ::std::ffi::c_char
        ) -> *const ::std::ffi::c_char {
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
