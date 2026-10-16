#[repr(transparent)]
pub struct Inner(pub (u8, bool));
#[repr(transparent)]
pub struct Outer(pub Inner);
pub fn puzzle(value: Outer) -> Outer {
    Outer(Inner((value.0.0.0, !value.0.0.1)))
}
