//! Voice transport encryption (`aead_aes256_gcm_rtpsize` and
//! `aead_xchacha20_poly1305_rtpsize`).
//!
//! Packet layout: `RTP header (+ext preamble) | ciphertext | tag(16) | nonce(4)`.
//! The header is authenticated as AAD; the 32-bit nonce counter is appended
//! in the clear and zero-padded to the cipher's nonce length.

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::Aes256Gcm;
use chacha20poly1305::XChaCha20Poly1305;

pub const TAG_LEN: usize = 16;
pub const NONCE_SUFFIX_LEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoMode {
    Aes256Gcm,
    XChaCha20,
}

impl CryptoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aead_aes256_gcm_rtpsize",
            Self::XChaCha20 => "aead_xchacha20_poly1305_rtpsize",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "aead_aes256_gcm_rtpsize" => Some(Self::Aes256Gcm),
            "aead_xchacha20_poly1305_rtpsize" => Some(Self::XChaCha20),
            _ => None,
        }
    }

    /// AES-GCM is hardware accelerated nearly everywhere, so prefer it.
    pub fn negotiate<'a>(modes: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let modes: Vec<&str> = modes.into_iter().collect();
        [Self::Aes256Gcm, Self::XChaCha20]
            .into_iter()
            .find(|m| modes.contains(&m.as_str()))
    }
}

pub enum Cipher {
    Aes(Box<Aes256Gcm>),
    XChaCha(Box<XChaCha20Poly1305>),
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("invalid key length")]
    Key,
    #[error("packet too short")]
    Short,
    #[error("authentication failed")]
    Auth,
}

impl Cipher {
    pub fn new(mode: CryptoMode, key: &[u8]) -> Result<Self, CryptoError> {
        Ok(match mode {
            CryptoMode::Aes256Gcm => Cipher::Aes(Box::new(Aes256Gcm::new_from_slice(key).map_err(|_| CryptoError::Key)?)),
            CryptoMode::XChaCha20 => {
                Cipher::XChaCha(Box::new(XChaCha20Poly1305::new_from_slice(key).map_err(|_| CryptoError::Key)?))
            }
        })
    }

    pub fn mode(&self) -> CryptoMode {
        match self {
            Cipher::Aes(_) => CryptoMode::Aes256Gcm,
            Cipher::XChaCha(_) => CryptoMode::XChaCha20,
        }
    }

    /// `buf` holds `aad | plaintext`; encrypts in place and appends tag + nonce.
    pub fn seal(&self, buf: &mut Vec<u8>, aad_len: usize, counter: u32) -> Result<(), CryptoError> {
        let (aad, body) = buf.split_at_mut(aad_len);
        let tag: [u8; TAG_LEN] = match self {
            Cipher::Aes(c) => {
                let mut nonce = aes_gcm::Nonce::default();
                nonce[..4].copy_from_slice(&counter.to_be_bytes());
                c.encrypt_in_place_detached(&nonce, aad, body).map_err(|_| CryptoError::Auth)?.into()
            }
            Cipher::XChaCha(c) => {
                let mut nonce = chacha20poly1305::XNonce::default();
                nonce[..4].copy_from_slice(&counter.to_be_bytes());
                c.encrypt_in_place_detached(&nonce, aad, body).map_err(|_| CryptoError::Auth)?.into()
            }
        };
        buf.extend_from_slice(&tag);
        buf.extend_from_slice(&counter.to_be_bytes());
        Ok(())
    }

    /// Decrypt in place. Returns the plaintext range within `packet`.
    pub fn open(&self, packet: &mut [u8], aad_len: usize) -> Result<std::ops::Range<usize>, CryptoError> {
        if packet.len() < aad_len + TAG_LEN + NONCE_SUFFIX_LEN {
            return Err(CryptoError::Short);
        }
        let len = packet.len();
        let mut counter = [0u8; 4];
        counter.copy_from_slice(&packet[len - 4..]);
        let body_end = len - NONCE_SUFFIX_LEN - TAG_LEN;
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&packet[body_end..body_end + TAG_LEN]);
        let (aad, rest) = packet.split_at_mut(aad_len);
        let body = &mut rest[..body_end - aad_len];
        match self {
            Cipher::Aes(c) => {
                let mut nonce = aes_gcm::Nonce::default();
                nonce[..4].copy_from_slice(&counter);
                c.decrypt_in_place_detached(&nonce, aad, body, (&tag).into())
            }
            Cipher::XChaCha(c) => {
                let mut nonce = chacha20poly1305::XNonce::default();
                nonce[..4].copy_from_slice(&counter);
                c.decrypt_in_place_detached(&nonce, aad, body, (&tag).into())
            }
        }
        .map_err(|_| CryptoError::Auth)?;
        Ok(aad_len..body_end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip_both_modes() {
        for mode in [CryptoMode::Aes256Gcm, CryptoMode::XChaCha20] {
            let c = Cipher::new(mode, &[7u8; 32]).unwrap();
            let mut buf = vec![0x80, 0x78, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3];
            buf.extend_from_slice(b"opus frame");
            c.seal(&mut buf, 12, 99).unwrap();
            assert_eq!(buf.len(), 12 + 10 + 16 + 4);
            let r = c.open(&mut buf, 12).unwrap();
            assert_eq!(&buf[r], b"opus frame");
        }
    }

    #[test]
    fn tamper_fails() {
        let c = Cipher::new(CryptoMode::Aes256Gcm, &[1u8; 32]).unwrap();
        let mut buf = vec![0u8; 12];
        buf.extend_from_slice(b"hello");
        c.seal(&mut buf, 12, 1).unwrap();
        buf[3] ^= 1; // header is authenticated
        assert!(c.open(&mut buf, 12).is_err());
    }

    #[test]
    fn negotiation_prefers_aes() {
        assert_eq!(
            CryptoMode::negotiate(["aead_xchacha20_poly1305_rtpsize", "aead_aes256_gcm_rtpsize"]),
            Some(CryptoMode::Aes256Gcm)
        );
        assert_eq!(CryptoMode::negotiate(["xsalsa20_poly1305"]), None);
    }
}
