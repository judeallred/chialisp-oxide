//! macOS's native readpassphrase restores terminal echo before delivering its
//! intercepted termination/job-control signals. No stdin fallback is permitted.
use super::{MasterSecret, Result, WalletError};

#[cfg(target_os = "macos")]
pub(super) fn read_master_secret() -> Result<MasterSecret> {
    use std::sync::Mutex;
    use zeroize::Zeroizing;
    static TERMINAL: Mutex<()> = Mutex::new(());
    let _terminal = TERMINAL.lock().map_err(|_| WalletError::TtyUnavailable)?;
    unsafe extern "C" {
        fn readpassphrase(
            prompt: *const libc::c_char,
            buffer: *mut libc::c_char,
            size: libc::size_t,
            flags: libc::c_int,
        ) -> *mut libc::c_char;
    }
    // MacOS SDK readpassphrase.h: ECHO_OFF=0, REQUIRE_TTY=2. The 65th retained
    // character detects truncation; embedded NULs cannot hide appended input
    // because the unchanged sentinel tail locates the actual written terminator.
    let mut text = Zeroizing::new([0xffu8; 66]);
    let returned = unsafe {
        readpassphrase(
            c"Disposable Testnet11 master secret (hidden): ".as_ptr(),
            text.as_mut_ptr().cast(),
            text.len(),
            2,
        )
    };
    if returned.is_null() {
        return Err(WalletError::TtyUnavailable);
    }
    parse_secret(&text)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn read_master_secret() -> Result<MasterSecret> {
    Err(WalletError::TtyUnavailable)
}

#[cfg(any(target_os = "macos", test))]
fn parse_secret(text: &[u8; 66]) -> Result<MasterSecret> {
    use chia_bls::SecretKey;
    use zeroize::Zeroizing;
    let end = text
        .iter()
        .rposition(|byte| *byte != 0xff)
        .ok_or(WalletError::SecretInput)?;
    if end != 64 || text[64] != 0 || !text[..64].iter().all(u8::is_ascii_hexdigit) {
        return Err(WalletError::SecretInput);
    }
    let mut decoded = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(&text[..64], decoded.as_mut()).map_err(|_| WalletError::SecretInput)?;
    let key = SecretKey::from_bytes(&decoded).map_err(|_| WalletError::SecretInput)?;
    if key.public_key().is_inf() {
        return Err(WalletError::SecretInput);
    }
    Ok(MasterSecret(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(input: &[u8]) -> [u8; 66] {
        let mut output = [0xff; 66];
        let count = input.len().min(65);
        output[..count].copy_from_slice(&input[..count]);
        output[count] = 0;
        output
    }
    #[test]
    fn bounded_input_rejects_truncation_nul_nonhex_and_invalid_scalar() {
        for value in [
            vec![],
            vec![b'1'; 63],
            vec![b'1'; 65],
            vec![b'g'; 64],
            vec![b'0'; 64],
            vec![b'f'; 64],
            [vec![b'1'; 64], vec![0, b'2']].concat(),
            [vec![b'1'; 32], vec![0], vec![b'1'; 31]].concat(),
        ] {
            assert!(matches!(
                parse_secret(&text(&value)),
                Err(WalletError::SecretInput)
            ));
        }
    }
    #[test]
    fn only_test_fixture_scalar_is_parsed_without_tty() {
        let fixture = chia_bls::SecretKey::from_seed(&[0x31; 32]);
        let encoded = hex::encode(fixture.to_bytes());
        let parsed = parse_secret(&text(encoded.as_bytes())).unwrap();
        assert_eq!(parsed.0.public_key(), fixture.public_key());
    }
}
