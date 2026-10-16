#[repr(transparent)]
pub struct Wrapper(pub [u8; 0]);
pub fn puzzle(value: Wrapper) -> Wrapper { value }
