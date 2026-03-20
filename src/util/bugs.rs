//! utilities for tracking down bugs in the code

#[track_caller]
pub fn internal_bug_fmt(args: std::fmt::Arguments<'_>) -> ! {
    panic!("internal compiler bug: {}", args);
}

#[macro_export]
macro_rules! internal_bug {
    ($($arg:tt)*) => {
        $crate::internal_bug_fmt(format_args!($($arg)*))
    };
}
