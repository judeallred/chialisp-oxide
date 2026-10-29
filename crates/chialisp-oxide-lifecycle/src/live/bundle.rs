use super::*;

/// Streamable vectors carry declared lengths. Bound every count and program
/// slice before invoking their allocator-backed decoder on retained bytes.
pub(super) fn decode_bundle(bytes: &[u8]) -> Result<SpendBundle> {
    ensure!(
        bytes.len() <= MAX_BUNDLE && bytes.len() >= 4 + 96,
        "bounded serialized bundle"
    );
    let count = u32::from_be_bytes(bytes[..4].try_into()?) as usize;
    ensure!((1..=2).contains(&count), "bounded bundle spend count");
    let mut cursor = 4usize;
    for _ in 0..count {
        cursor = cursor.checked_add(72).context("coin offset overflow")?;
        ensure!(cursor <= bytes.len(), "truncated coin");
        for _ in 0..2 {
            scan_plain_program(bytes, &mut cursor)?;
        }
    }
    ensure!(
        cursor.checked_add(96) == Some(bytes.len()),
        "exact bundle signature/trailing bytes"
    );
    Ok(SpendBundle::from_bytes(bytes)?)
}

/// CLVM Programs are self-delimiting in Streamable, not length-prefixed.
/// Preflight the complete plain tree with bounded iterative work/depth before
/// the SDK parser copies it. Backreferences are outside this emitted profile.
fn scan_plain_program(bytes: &[u8], cursor: &mut usize) -> Result<()> {
    let start = *cursor;
    let mut stack = vec![1u8];
    let mut nodes = 0usize;
    while let Some(remaining) = stack.last_mut() {
        if *remaining == 0 {
            stack.pop();
            continue;
        }
        *remaining -= 1;
        nodes += 1;
        ensure!(nodes <= 65_536, "bundle program node bound");
        let byte = *bytes.get(*cursor).context("truncated program")?;
        *cursor += 1;
        if byte == 0xff {
            ensure!(stack.len() < 512, "bundle program depth bound");
            stack.push(2);
        } else if byte > 0x7f {
            ensure!(byte != 0xfe, "backreferenced bundle program unsupported");
            let prefix = byte.leading_ones() as usize;
            ensure!((1..=6).contains(&prefix), "invalid atom length prefix");
            let mut length = usize::from(byte & ((1u8 << (7 - prefix)) - 1));
            for _ in 1..prefix {
                length = length
                    .checked_mul(256)
                    .context("atom length overflow")?
                    .checked_add(usize::from(
                        *bytes.get(*cursor).context("truncated atom length")?,
                    ))
                    .context("atom length overflow")?;
                *cursor += 1;
            }
            ensure!(length <= MAX_BUNDLE, "bundle atom byte bound");
            *cursor = cursor.checked_add(length).context("atom offset overflow")?;
            ensure!(*cursor <= bytes.len(), "truncated atom");
        }
        ensure!(*cursor - start <= MAX_BUNDLE, "bundle program byte bound");
    }
    Ok(())
}
