#[repr(transparent)]
pub struct Wrapper(pub [u8; 32], pub ());
pub fn puzzle(value: Wrapper) -> Wrapper {
    value
}
