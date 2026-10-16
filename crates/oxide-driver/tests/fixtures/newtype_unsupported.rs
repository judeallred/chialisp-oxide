#[repr(transparent)]
pub struct Wrapper(pub f64);
pub fn puzzle(value: Wrapper) -> Wrapper { value }
