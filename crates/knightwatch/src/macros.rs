#[macro_export]
macro_rules! all_none {
    ($($opt:expr),+ $(,)?) => {
        $($opt.is_none())&&+
    };
}
