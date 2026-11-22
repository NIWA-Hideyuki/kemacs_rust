use std::io::Read;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::Sha256;

/// バッファの暗号化種別。Classic は C 版互換 (研究用)、Strong は AEAD。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CryptKind {
    #[default]
    None,
    Classic,
    Strong,
}

pub const STRONG_MAGIC: &[u8] = b"KEMACS-STRONG-V1\0";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const PBKDF2_ROUNDS: u32 = 100_000;

fn derive_key(password: &[u8], salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password, salt, PBKDF2_ROUNDS, &mut key);
    key
}

fn random_bytes(buf: &mut [u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::open("/dev/urandom")?;
    f.read_exact(buf)
}

/// XChaCha20-Poly1305 + PBKDF2-SHA256 による認証付き暗号化。
/// 形式: MAGIC | salt(16) | nonce(24) | ciphertext(tag を含む)。
pub fn strong_encrypt(plain: &[u8], password: &[u8]) -> Option<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut salt).ok()?;
    random_bytes(&mut nonce).ok()?;
    let cipher = XChaCha20Poly1305::new_from_slice(&derive_key(password, &salt)).ok()?;
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: b"",
            },
        )
        .ok()?;
    let mut out = Vec::with_capacity(STRONG_MAGIC.len() + SALT_LEN + NONCE_LEN + ct.len());
    out.extend_from_slice(STRONG_MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Some(out)
}

pub fn strong_decrypt(data: &[u8], password: &[u8]) -> Option<Vec<u8>> {
    let prefix = STRONG_MAGIC.len() + SALT_LEN + NONCE_LEN;
    if data.len() < prefix + 16 || !data.starts_with(STRONG_MAGIC) {
        return None;
    }
    let salt = &data[STRONG_MAGIC.len()..STRONG_MAGIC.len() + SALT_LEN];
    let nonce_at = STRONG_MAGIC.len() + SALT_LEN;
    let nonce = &data[nonce_at..nonce_at + NONCE_LEN];
    let cipher = XChaCha20Poly1305::new_from_slice(&derive_key(password, salt)).ok()?;
    cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: &data[prefix..],
                aad: b"",
            },
        )
        .ok()
}

// MicroEMACS の crypt.C_ (DLH-POLY-86-B CIPHER) の Rust 版。
// 印字可能文字 (' '..'~') のみを Beaufort 暗号化し、制御文字・高位バイトは
// そのまま通す。暗号化と復号は同じ処理 (isomorphic)。

#[derive(Clone, Debug)]
pub struct Cipher {
    key: i64,
    salt: i64,
}

fn mod95(mut v: i64) -> i64 {
    while v >= 9500 {
        v -= 9500;
    }
    while v >= 950 {
        v -= 950;
    }
    while v >= 95 {
        v -= 95;
    }
    while v < 0 {
        v += 95;
    }
    v
}

impl Cipher {
    /// C 版 cryptstr(NULL, vector) に相当。
    pub fn new(vector: i64) -> Self {
        Cipher {
            key: vector,
            salt: vector,
        }
    }

    /// C 版 cryptstr(ptr, len) に相当 (in-place)。
    pub fn apply(&mut self, data: &mut [u8]) {
        for c in data.iter_mut() {
            if !(b' '..=b'~').contains(c) {
                continue;
            }
            self.key &= 0x1fff_ffff;
            if self.key & 0x1000_0000 != 0 {
                self.key ^= 0x0040_a001;
            }
            let clear = *c;
            *c = (mod95(self.key % 95 - (clear as i64 - b' ' as i64)) + b' ' as i64) as u8;
            self.salt += 1;
            if self.salt >= 20857 {
                self.salt = 0;
            }
            self.key = (self.key * 2 + *c as i64 + clear as i64 + self.salt) & 0x1fff_ffff;
        }
    }

    /// setcrkey が保存する暗号化済みキーを作る。
    pub fn prepare_key(plaintext: &[u8]) -> Vec<u8> {
        let mut c = Cipher::new(0);
        let mut key = plaintext.to_vec();
        c.apply(&mut key);
        key
    }

    /// 保存済みキーからファイル I/O 用の暗号状態を作る。
    pub fn from_key(encrypted_key: &[u8]) -> Self {
        let mut c = Cipher::new(0);
        let mut k = encrypted_key.to_vec();
        c.apply(&mut k);
        c
    }

    pub fn crypt(&mut self, data: &mut [u8]) {
        self.apply(data);
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        self.apply(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isomorphic_cipher() {
        let key = Cipher::prepare_key(b"secret");
        let plain = b"Hello, kemacs! 123\n\t\x82\xa0";
        let mut enc = plain.to_vec();
        Cipher::from_key(&key).crypt(&mut enc);
        assert_ne!(enc, plain);
        // 制御文字と高位バイトは変化しない
        assert_eq!(enc[enc.len() - 3], b'\t');
        assert_eq!(enc[enc.len() - 2], 0x82);
        let mut dec = enc.clone();
        Cipher::from_key(&key).decrypt(&mut dec);
        assert_eq!(dec, plain);
    }

    #[test]
    fn strong_roundtrip_and_tamper_detection() {
        let pass = b"correct horse battery staple";
        let plain = "secret 日本語\nline2".as_bytes();
        let enc = strong_encrypt(plain, pass).unwrap();
        assert!(enc.starts_with(STRONG_MAGIC));
        assert_ne!(enc, plain);
        assert_eq!(strong_decrypt(&enc, pass).unwrap(), plain);
        assert!(strong_decrypt(&enc, b"wrong password").is_none());
        let mut bad = enc.clone();
        let n = bad.len() - 1;
        bad[n] ^= 1;
        assert!(strong_decrypt(&bad, pass).is_none());
        assert!(strong_decrypt(b"not encrypted", pass).is_none());
    }

    #[test]
    fn key_changes_ciphertext() {
        let a = Cipher::prepare_key(b"short");
        let b = Cipher::prepare_key(b"a much longer key");
        let mut ea = b"same text".to_vec();
        let mut eb = ea.clone();
        Cipher::from_key(&a).crypt(&mut ea);
        Cipher::from_key(&b).crypt(&mut eb);
        assert_ne!(ea, eb);
    }
}
