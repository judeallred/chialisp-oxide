#![forbid(unsafe_code)]
//! Exact atom-payload hashing for native tests and the reviewed compiler bridge.
//! This module does not establish authorization, domain separation or security.

/// SHA256 of the exact bytes, returning all 32 digest bytes, including zeros.
///
/// No integer, UTF-8, hexadecimal or CLVM serialization conversion is performed.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    sha256_parts(bytes, &[], &[])
}

/// Scalar SHA-256 of the exact concatenation a || b || c.
///
/// Native kernel for the prototype compiler bridge; not a verified implementation.
/// Byte/block positions use u128: three usize lengths plus SHA padding fit even
/// on a 64-bit target. FIPS 180-4 defines SHA-256 for messages below 2^64 bits;
/// this implementation encodes the low 64 bits of the bit length explicitly.
/// No allocation, helper/trait calls, architecture dispatch or wrapping methods.
// Keep the reviewed scalar operations explicit: rotate/div_ceil methods would
// change the closed MIR call graph that the compiler bridge matches.
#[allow(clippy::manual_rotate, clippy::manual_div_ceil)]
pub fn sha256_parts(a: &[u8], b: &[u8], c: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let a_len = a.len() as u128;
    let ab_len = a_len + b.len() as u128;
    let message_len = ab_len + c.len() as u128;
    let bit_len = (message_len << 3) as u64;
    let padded_len = ((message_len + 9 + 63) / 64) * 64;
    let mut block_start = 0u128;
    while block_start < padded_len {
        let mut w = [0u32; 64];
        let mut i = 0usize;
        while i < 64 {
            let position = block_start + i as u128;
            let byte = if position < a_len {
                a[position as usize]
            } else if position < ab_len {
                b[(position - a_len) as usize]
            } else if position < message_len {
                c[(position - ab_len) as usize]
            } else if position == message_len {
                0x80
            } else if position >= padded_len - 8 {
                (bit_len >> ((padded_len - 1 - position) as u32 * 8)) as u8
            } else {
                0
            };
            let word = i / 4;
            w[word] = (w[word] << 8) | byte as u32;
            i += 1;
        }
        i = 16;
        while i < 64 {
            let x = w[i - 15];
            let y = w[i - 2];
            let small0 = ((x >> 7) | (x << 25)) ^ ((x >> 18) | (x << 14)) ^ (x >> 3);
            let small1 = ((y >> 17) | (y << 15)) ^ ((y >> 19) | (y << 13)) ^ (y >> 10);
            w[i] = (w[i - 16] as u64 + small0 as u64 + w[i - 7] as u64 + small1 as u64) as u32;
            i += 1;
        }
        let mut aa = h[0];
        let mut bb = h[1];
        let mut cc = h[2];
        let mut dd = h[3];
        let mut ee = h[4];
        let mut ff = h[5];
        let mut gg = h[6];
        let mut hh = h[7];
        i = 0;
        while i < 64 {
            let big1 =
                ((ee >> 6) | (ee << 26)) ^ ((ee >> 11) | (ee << 21)) ^ ((ee >> 25) | (ee << 7));
            let choose = (ee & ff) ^ (!ee & gg);
            let temp1 =
                (hh as u64 + big1 as u64 + choose as u64 + K[i] as u64 + w[i] as u64) as u32;
            let big0 =
                ((aa >> 2) | (aa << 30)) ^ ((aa >> 13) | (aa << 19)) ^ ((aa >> 22) | (aa << 10));
            let majority = (aa & bb) ^ (aa & cc) ^ (bb & cc);
            let temp2 = (big0 as u64 + majority as u64) as u32;
            hh = gg;
            gg = ff;
            ff = ee;
            ee = (dd as u64 + temp1 as u64) as u32;
            dd = cc;
            cc = bb;
            bb = aa;
            aa = (temp1 as u64 + temp2 as u64) as u32;
            i += 1;
        }
        h[0] = (h[0] as u64 + aa as u64) as u32;
        h[1] = (h[1] as u64 + bb as u64) as u32;
        h[2] = (h[2] as u64 + cc as u64) as u32;
        h[3] = (h[3] as u64 + dd as u64) as u32;
        h[4] = (h[4] as u64 + ee as u64) as u32;
        h[5] = (h[5] as u64 + ff as u64) as u32;
        h[6] = (h[6] as u64 + gg as u64) as u32;
        h[7] = (h[7] as u64 + hh as u64) as u32;
        block_start += 64;
    }
    let mut digest = [0u8; 32];
    let mut i = 0usize;
    while i < 32 {
        digest[i] = (h[i / 4] >> (24 - (i % 4) * 8)) as u8;
        i += 1;
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::{sha256, sha256_parts};

    #[test]
    fn known_answers_and_exact_multipart_payloads() {
        let empty = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];
        let abc = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(sha256(b""), empty);
        assert_eq!(sha256_parts(b"", b"", b""), empty);
        assert_eq!(sha256(b"abc"), abc);
        assert_eq!(sha256_parts(b"a", b"b", b"c"), abc);
        assert_eq!(sha256_parts(b"", b"abc", b""), abc);
        assert_ne!(sha256(b""), sha256(b"\0"));
        assert_ne!(sha256(b"\x01"), sha256(b"\0\x01"));
    }
}
