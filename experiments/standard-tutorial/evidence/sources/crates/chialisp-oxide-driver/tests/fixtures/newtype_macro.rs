macro_rules! wrapper {
    () => {
        #[repr(transparent)]
        pub struct Wrapper(pub u64);
    };
}
wrapper!();
pub fn puzzle(value: Wrapper) -> Wrapper {
    value
}
