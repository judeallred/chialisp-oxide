pub fn puzzle(amount: u64) -> u64 {
    let mut value = amount;
    let first = &mut value;
    let second = &mut value;
    *first + *second
}
