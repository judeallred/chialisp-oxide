#[repr(transparent)]
pub struct Wrapper<T>(pub T);
pub fn puzzle(value: Wrapper<u64>) -> Wrapper<u64> { value }
