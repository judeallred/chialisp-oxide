#[repr(transparent)]
pub struct Wrapper(pub std::num::NonZeroU64);
pub fn puzzle(value: Wrapper) -> Wrapper { value }
