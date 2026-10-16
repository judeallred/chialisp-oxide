#[repr(transparent)]
pub struct Wrapper(pub u64);
impl Drop for Wrapper { fn drop(&mut self) {} }
pub fn puzzle(value: Wrapper) -> u64 { value.0 }
