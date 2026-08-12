//! Verification for the signed release channel.
//!
//! Networking and installation belong to the frontend because both are UI-
//! and platform-shaped. The trust decision does not: every frontend asks the
//! same compiled-in Ed25519 key whether bytes are a Sterna release artifact.

use ed25519_dalek::{Signature, VerifyingKey};

include!(concat!(env!("OUT_DIR"), "/update_key.rs"));

pub(crate) fn verify(data: &[u8], signature: &[u8]) -> bool {
    verify_with_key(UPDATE_PUBLIC_KEY, data, signature)
}

fn verify_with_key(key: [u8; 32], data: &[u8], signature: &[u8]) -> bool {
    let Ok(key) = VerifyingKey::from_bytes(&key) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(signature) else {
        return false;
    };
    key.verify_strict(data, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex<const N: usize>(text: &str) -> [u8; N] {
        assert_eq!(text.len(), N * 2);
        let mut out = [0; N];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    fn base64(text: &str) -> Vec<u8> {
        fn value(byte: u8) -> u8 {
            match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => panic!("invalid base64 fixture"),
            }
        }
        let text = text.trim().as_bytes();
        let mut out = Vec::new();
        for chunk in text.chunks_exact(4) {
            let a = value(chunk[0]);
            let b = value(chunk[1]);
            let c = (chunk[2] != b'=').then(|| value(chunk[2]));
            let d = (chunk[3] != b'=').then(|| value(chunk[3]));
            out.push((a << 2) | (b >> 4));
            if let Some(c) = c {
                out.push((b << 4) | (c >> 2));
                if let Some(d) = d {
                    out.push((c << 6) | d);
                }
            }
        }
        out
    }

    #[test]
    fn rfc8032_vector_verifies_and_tampering_does_not() {
        // RFC 8032 section 7.1, TEST 1: an empty message.
        let key = hex(concat!(
            "d75a980182b10ab7d54bfed3c964073a",
            "0ee172f3daa62325af021a68f707511a"
        ));
        let signature: [u8; 64] = hex(concat!(
            "e5564300c360ac729086e2cc806e828a",
            "84877f1eb8e5d974d873e06522490155",
            "5fb8821590a33bacc61e39701cf9b46b",
            "d25bf5f0595bbe24655141438e7a100b"
        ));
        assert!(verify_with_key(key, b"", &signature));
        assert!(!verify_with_key(key, b"x", &signature));
        assert!(!verify_with_key(key, b"", &signature[..63]));
    }

    #[test]
    fn committed_signer_matches_the_compiled_key() {
        let message = include_bytes!("../../../packaging/update/test-message.txt");
        let signature = base64(include_str!("../../../packaging/update/test-signature.txt"));
        assert!(verify(message, &signature));

        let mut changed = message.to_vec();
        changed[0] ^= 1;
        assert!(!verify(&changed, &signature));
    }
}
