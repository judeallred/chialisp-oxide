#[repr(transparent)]
pub struct Count(pub u64);
pub fn puzzle(mut value: Count) -> Count {
    value.0 += 1;
    value
}
