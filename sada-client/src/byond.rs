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

use meowtonin::{
    ByondValue,
    sys::{CByondValue, u4c},
};

thread_local! {
    /// Stores the most recent string returned to BYOND on this thread.
    ///
    /// BYOND expects the returned C string pointer to remain valid after the
    /// exported function returns. Keeping the allocation in thread-local storage
    /// gives each call a stable pointer until the next string return on the same
    /// thread replaces it.
    #[doc(hidden)]
    pub static LAST_RETURN: RefCell<CString> = Default::default();
}

/// Shared empty C string used for BYOND functions that return no value.
#[doc(hidden)]
pub static VOID_RETURN: c_char = 0;

/// Converts one raw BYOND argument into a Rust parameter value.
#[doc(hidden)]
pub trait FromByondArg<'a>: Sized {
    /// Convert `arg` into the destination type.
    fn from_byond_arg(arg: &'a str) -> Self;
}

impl<'a> FromByondArg<'a> for &'a str {
    fn from_byond_arg(arg: &'a str) -> Self { arg }
}

/// Implement BYOND argument conversion for owned types parsed from strings.
macro_rules! impl_from_str_byond_arg {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<'a> FromByondArg<'a> for $ty {
                fn from_byond_arg(arg: &'a str) -> Self {
                    ::std::str::FromStr::from_str(arg).unwrap_or_default()
                }
            }
        )*
    };
}

impl_from_str_byond_arg!(
    String, bool, char, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64
);

/// Convert BYOND's raw argument array into borrowed Rust string slices.
///
/// Missing arguments are returned as empty strings. Arguments that are not valid
/// UTF-8 are also treated as empty strings, matching the macro's forgiving
/// conversion behavior.
///
/// # Safety
///
/// `argv` must point to an array containing at least `argc` valid, non-null
/// pointers to NUL-terminated C strings. Each pointed-to string must remain
/// valid for the returned lifetime.
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

/// Install the process-wide panic hook used by BYOND exports.
///
/// The hook preserves the previously installed panic hook, then appends the
/// panic payload and captured backtrace to `sada_panic.log`.
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

/// Returns the number of arguments passed to the macro.
#[doc(hidden)]
macro_rules! __count_args {
    () => { 0 };
    ($head:ident $(, $tail:ident)*) => { 1 + $crate::byond::__count_args!($($tail),*) };
}

/// Define a BYOND-compatible exported function.
///
/// The macro wraps a Rust function body in an `unsafe extern "C"` function with
/// BYOND's `argc`/`argv` calling convention, parses arguments, installs the panic
/// hook by default, and converts the return value to a C string pointer.
macro_rules! function {
    (@panic_hook) => {
        $crate::byond::__set_panic_hook();
    };
    (@panic_hook $skip_hook:literal) => {
        if !$skip_hook { $crate::byond::__set_panic_hook(); }
    };

    (@parse_args $argc:ident, $argv:ident;) => {};
    (@parse_args $argc:ident, $argv:ident; $($arg:ident : $arg_ty:ty),+) => {
        let __args = unsafe { $crate::byond::__parse_args::<{ $crate::byond::__count_args!($($arg),*) }>($argc, $argv) };
        let mut __argi = 0;
        $(let $arg: $arg_ty = <$arg_ty as $crate::byond::FromByondArg>::from_byond_arg(__args[__argi]); __argi += 1;)*
    };

    (@return $res:ident ->) => {
        &$crate::byond::VOID_RETURN
    };
    (@return $res:ident -> $ret:ty) => {{
        $crate::byond::LAST_RETURN.with(|last| {
            last.replace(::std::ffi::CString::new(<$ret as ToString>::to_string(&$res)).unwrap_or_default());
            last.borrow().as_ptr()
        })
    }};

    attr($(skip_hook = $skip_hook:literal)?) (fn $name:ident($($arg:ident : $arg_ty:ty),* $(,)?) $(-> $ret:ty)? $body:block) => {
        #[unsafe(no_mangle)]
        #[allow(missing_docs, clippy::missing_safety_doc)]
        pub unsafe extern "C" fn $name(
            _argc: ::std::ffi::c_int, _argv: *const *const ::std::ffi::c_char
        ) -> *const ::std::ffi::c_char {
            $crate::byond::function!(@panic_hook $($skip_hook)?);
            $crate::byond::function!(@parse_args _argc, _argv; $($arg : $arg_ty),*);
            let __result = $body;
            $crate::byond::function!(@return __result -> $($ret)?)
        }
    };
}

/// todo
pub fn __parse_args2<const N: usize>(argc: u4c, argv: *mut CByondValue) -> [ByondValue; N] {
    let mut args = [ByondValue::NULL; N];
    for (i, arg) in args.iter_mut().enumerate().take((argc as usize).min(N)) {
        *arg = unsafe { ByondValue(*argv.add(i)) };
    }
    args
}

/// todo
macro_rules! byondapi {
    (@panic_hook) => {
        $crate::byond::__set_panic_hook();
    };
    (@panic_hook $skip_hook:literal) => {
        if !$skip_hook { $crate::byond::__set_panic_hook(); }
    };

    (@parse_args $argc:ident, $argv:ident,) => {};
    (@parse_args $argc:ident, $argv:ident, $($arg:ident : $arg_ty:ty),+) => {
        let [$($arg),*] = $crate::byond::__parse_args2::<{ $crate::byond::__count_args!($($arg),*) }>($argc, $argv);
        $(let $arg: $arg_ty = <$arg_ty as ::meowtonin::FromByond>::from_byond($arg).unwrap_or_default();)*
    };

    (@return $res:ident ->) => {
        ::meowtonin::ByondValue::NULL.detach()
    };
    (@return $res:ident -> $ret:ty) => {{
        ::meowtonin::ByondValue::new_value::<$ret>($res).unwrap_or_default().detach()
    }};

    // attr($(catch = $catch:literal)?)

    attr() (fn $name:ident($($arg:ident : $arg_ty:ty),* $(,)?) $(-> $ret:ty)? $body:block) => {
        #[unsafe(no_mangle)]
        #[allow(missing_docs, clippy::missing_safety_doc)]
        pub unsafe extern "C-unwind" fn $name(
            __argc: ::meowtonin::sys::u4c, __argv: *mut ::meowtonin::sys::CByondValue,
        ) -> ::meowtonin::sys::CByondValue {
            // $crate::byond::byondapi!(@panic_hook $($skip_hook)?);
            $crate::byond::byondapi!(@parse_args __argc, __argv, $($arg : $arg_ty),*);
            let __result = $body;
            $crate::byond::byondapi!(@return __result -> $($ret)?)
        }
    };
}

pub(crate) use __count_args;
pub(crate) use byondapi;
#[doc(inline)]
pub(crate) use function;
