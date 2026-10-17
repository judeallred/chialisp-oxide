//! Native execution of the public implementation, independent of CLVM lowering.
//! RustCrypto is also used by clvmr: differential agreement with it is not an
//! independent VM crypto implementation. Fixed published known answers provide
//! another oracle; neither finite suite proves SHA-256 or compiler preservation.
use chialisp_oxide::{sha256, sha256_parts};
use sha2::{Digest, Sha256};

fn digest(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64);
    core::array::from_fn(|index| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap())
}

#[test]
fn published_known_answers_cover_single_and_multiple_blocks() {
    for (input, expected) in [
        (
            &b""[..],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            &b"abc"[..],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            &b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"[..],
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ] {
        let expected = digest(expected);
        assert_eq!(sha256(input), expected);
        assert_eq!(sha256_parts(&[], input, &[]), expected);
    }
    assert_eq!(
        sha256(&vec![b'a'; 1_000_000]),
        digest("cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0")
    );
}

#[test]
fn every_short_partition_preserves_exact_concatenation_and_padding() {
    // Every pair of split positions, including empty parts, across the first
    // SHA padding transition. Expected bytes come from a separate implementation.
    for length in 0..=70 {
        let input: Vec<_> = (0..length).map(|i| (i * 37 + length * 13) as u8).collect();
        let expected: [u8; 32] = Sha256::digest(&input).into();
        assert_eq!(sha256(&input), expected, "length {length}");
        for first in 0..=length {
            for second in first..=length {
                assert_eq!(
                    sha256_parts(&input[..first], &input[first..second], &input[second..]),
                    expected,
                    "length {length}, boundaries {first}/{second}"
                );
            }
        }
    }
}

#[test]
fn deterministic_binary_messages_cover_larger_padding_boundaries() {
    let mut seed = 0x60b1_e37a_82c4_590du64;
    for length in [
        119, 120, 121, 127, 128, 129, 183, 184, 185, 191, 192, 193, 255, 256, 257, 511, 512, 513,
        1023, 1024, 1025, 4095, 4096, 4097, 65_535, 65_536, 65_537,
    ] {
        let input: Vec<_> = (0..length)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        let expected: [u8; 32] = Sha256::digest(&input).into();
        for (first, second) in [
            (0, length),
            (length, length),
            (0, 0),
            (length / 3, length * 2 / 3),
            (55, 56),
            (63, 64),
        ] {
            assert_eq!(
                sha256_parts(&input[..first], &input[first..second], &input[second..]),
                expected,
                "length {length}, boundaries {first}/{second}"
            );
        }
    }
}

#[test]
fn payload_and_digest_zeros_are_not_integer_encoding() {
    assert_ne!(sha256(&[]), sha256(&[0]));
    assert_ne!(sha256(&[1]), sha256(&[0, 1]));
    assert_ne!(sha256(&[0xff]), sha256(&[0, 0xff]));
    // Independently generated with Python hashlib / OpenSSL for bytes 00 5c.
    let expected = digest("00bc4b668ccce3ae825dbc18d6ff1dcdd9ec14b284c4f161fa5cfbb2d5069534");
    assert_eq!(sha256(&[0, 0x5c]), expected);
    assert_eq!(sha256_parts(&[0], &[], &[0x5c]), expected);
    assert_eq!(
        sha256_parts(b"a", b"bc", &[]),
        sha256_parts(b"ab", &[], b"c")
    );
}
