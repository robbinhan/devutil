//! Digests for text and for files read in chunks.
//!
//! Plain mode reports MD5, SHA-1, SHA-224, SHA-256, SHA-384, SHA-512,
//! SHA3-256, BLAKE3, and CRC32 together. HMAC mode uses the same RustCrypto
//! hashes. BLAKE3's keyed mode is not HMAC, and CRC32 has no HMAC, so those
//! two rows are unavailable while a key is set.

use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use blake3::Hasher as Blake3Hasher;
use digest::{Digest, KeyInit};
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::Sha1;
use sha2::{Sha224, Sha256, Sha384, Sha512};
use sha3::Sha3_256;

/// Bytes read into memory at a time. A multi-gigabyte file stays at about
/// one chunk plus the hasher state.
pub const CHUNK_SIZE: usize = 1024 * 1024;

const ALGORITHMS: [Algorithm; 9] = [
    Algorithm::Md5,
    Algorithm::Sha1,
    Algorithm::Sha224,
    Algorithm::Sha256,
    Algorithm::Sha384,
    Algorithm::Sha512,
    Algorithm::Sha3_256,
    Algorithm::Blake3,
    Algorithm::Crc32,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha3_256,
    Blake3,
    Crc32,
}

impl Algorithm {
    pub fn all() -> &'static [Algorithm] {
        &ALGORITHMS
    }

    /// HMAC via the `hmac` crate. BLAKE3 and CRC32 are not in that set.
    pub fn supports_hmac(self) -> bool {
        !matches!(self, Algorithm::Blake3 | Algorithm::Crc32)
    }

    pub fn label(self, hmac: bool) -> &'static str {
        match (hmac && self.supports_hmac(), self) {
            (false, Algorithm::Md5) => "MD5",
            (false, Algorithm::Sha1) => "SHA-1",
            (false, Algorithm::Sha224) => "SHA-224",
            (false, Algorithm::Sha256) => "SHA-256",
            (false, Algorithm::Sha384) => "SHA-384",
            (false, Algorithm::Sha512) => "SHA-512",
            (false, Algorithm::Sha3_256) => "SHA3-256",
            (false, Algorithm::Blake3) => "BLAKE3",
            (false, Algorithm::Crc32) => "CRC32",
            (true, Algorithm::Md5) => "HMAC-MD5",
            (true, Algorithm::Sha1) => "HMAC-SHA-1",
            (true, Algorithm::Sha224) => "HMAC-SHA-224",
            (true, Algorithm::Sha256) => "HMAC-SHA-256",
            (true, Algorithm::Sha384) => "HMAC-SHA-384",
            (true, Algorithm::Sha512) => "HMAC-SHA-512",
            (true, Algorithm::Sha3_256) => "HMAC-SHA3-256",
            (true, Algorithm::Blake3 | Algorithm::Crc32) => unreachable!("no HMAC label"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigestRow {
    pub algorithm: Algorithm,
    pub label: &'static str,
    /// Raw digest. Empty when [`Self::available`] is false.
    pub bytes: Vec<u8>,
    /// False when HMAC was requested and this algorithm has none.
    pub available: bool,
}

impl DigestRow {
    pub fn display(&self, uppercase: bool, base64: bool) -> String {
        if !self.available {
            return String::new();
        }
        if base64 {
            base64_encode(&self.bytes)
        } else {
            hex_encode(&self.bytes, uppercase)
        }
    }

    /// Case-insensitive match against the hex digest or its standard Base64.
    pub fn matches(&self, expected: &str) -> bool {
        if !self.available {
            return false;
        }
        let expected = normalize_expected(expected);
        if expected.is_empty() {
            return false;
        }
        let hex = hex_encode(&self.bytes, false);
        if hex.eq_ignore_ascii_case(&expected) {
            return true;
        }
        base64_encode(&self.bytes).eq_ignore_ascii_case(&expected)
    }
}

#[derive(Debug)]
pub enum HashError {
    Io(io::Error),
    Cancelled,
}

impl std::fmt::Display for HashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HashError::Io(err) => write!(f, "{err}"),
            HashError::Cancelled => write!(f, "hash cancelled"),
        }
    }
}

impl std::error::Error for HashError {}

impl From<io::Error> for HashError {
    fn from(err: io::Error) -> Self {
        HashError::Io(err)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHash {
    pub size: u64,
    pub rows: Vec<DigestRow>,
}

/// `key == None` is the plain digest. `Some` is HMAC, including an empty key.
pub fn hash_bytes(data: &[u8], key: Option<&[u8]>) -> Vec<DigestRow> {
    let mut set = HasherSet::new(key);
    set.update(data);
    set.finish()
}

pub fn hash_text(text: &str, key: Option<&[u8]>) -> Vec<DigestRow> {
    hash_bytes(text.as_bytes(), key)
}

/// Hash `path` without reading it into one buffer. `progress` is
/// `(bytes_read, size_from_metadata)` after each chunk. `cancel` is checked
/// between chunks; a set flag returns [`HashError::Cancelled`].
pub fn hash_path(
    path: &Path,
    key: Option<&[u8]>,
    progress: impl FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<FileHash, HashError> {
    let file = std::fs::File::open(path)?;
    let total = file.metadata()?.len();
    let rows = hash_read(file, total, key, CHUNK_SIZE, progress, cancel)?;
    Ok(FileHash { size: total, rows })
}

pub fn hash_read<R: Read>(
    reader: R,
    total: u64,
    key: Option<&[u8]>,
    chunk_size: usize,
    mut progress: impl FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<Vec<DigestRow>, HashError> {
    let chunk_size = chunk_size.max(1);
    let mut reader = reader;
    let mut buf = vec![0u8; chunk_size];
    let mut set = HasherSet::new(key);
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(HashError::Cancelled);
        }
        let mut filled = 0usize;
        while filled < buf.len() {
            let n = reader.read(&mut buf[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            break;
        }
        set.update(&buf[..filled]);
        done += filled as u64;
        progress(done, total);
    }
    if done == 0 {
        progress(0, total);
    }
    Ok(set.finish())
}

pub fn hex_encode(bytes: &[u8], uppercase: bool) -> String {
    const LOWER: &[u8; 16] = b"0123456789abcdef";
    const UPPER: &[u8; 16] = b"0123456789ABCDEF";
    let alphabet = if uppercase { UPPER } else { LOWER };
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(alphabet[(byte >> 4) as usize] as char);
        out.push(alphabet[(byte & 0x0f) as usize] as char);
    }
    out
}

pub fn base64_encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Trim, drop whitespace, and accept a leading `0x` or a single `algo:` label.
pub fn normalize_expected(expected: &str) -> String {
    let mut text = expected.trim();
    if let Some(rest) = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
    {
        text = rest.trim();
    }
    if let Some((label, rest)) = text.split_once(':') {
        let rest = rest.trim();
        if !label.chars().any(char::is_whitespace) && looks_like_digest(rest) {
            text = rest;
        }
    }
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn looks_like_digest(text: &str) -> bool {
    let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact.len() >= 8
        && compact
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || matches!(ch, '+' | '/' | '=' | '-' | '_'))
}

struct HasherSet {
    slots: Vec<Slot>,
}

struct Slot {
    algorithm: Algorithm,
    state: State,
}

enum State {
    Md5(Md5),
    Sha1(Sha1),
    Sha224(Sha224),
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
    Sha3(Sha3_256),
    HmacMd5(Hmac<Md5>),
    HmacSha1(Hmac<Sha1>),
    HmacSha224(Hmac<Sha224>),
    HmacSha256(Hmac<Sha256>),
    HmacSha384(Hmac<Sha384>),
    HmacSha512(Hmac<Sha512>),
    HmacSha3(Hmac<Sha3_256>),
    Blake3(Blake3Hasher),
    Crc32(crc32fast::Hasher),
    Unsupported,
}

impl HasherSet {
    fn new(key: Option<&[u8]>) -> Self {
        let slots = ALGORITHMS
            .into_iter()
            .map(|algorithm| Slot {
                algorithm,
                state: State::begin(algorithm, key),
            })
            .collect();
        Self { slots }
    }

    fn update(&mut self, data: &[u8]) {
        for slot in &mut self.slots {
            slot.state.update(data);
        }
    }

    fn finish(self) -> Vec<DigestRow> {
        let hmac = self.slots.iter().any(|slot| slot.state.is_hmac());
        self.slots
            .into_iter()
            .map(|slot| {
                let available = !matches!(slot.state, State::Unsupported);
                let bytes = slot.state.finish();
                DigestRow {
                    algorithm: slot.algorithm,
                    label: slot.algorithm.label(hmac),
                    bytes,
                    available,
                }
            })
            .collect()
    }
}

impl State {
    fn begin(algorithm: Algorithm, key: Option<&[u8]>) -> Self {
        let Some(key) = key else {
            return match algorithm {
                Algorithm::Md5 => State::Md5(Md5::new()),
                Algorithm::Sha1 => State::Sha1(Sha1::new()),
                Algorithm::Sha224 => State::Sha224(Sha224::new()),
                Algorithm::Sha256 => State::Sha256(Sha256::new()),
                Algorithm::Sha384 => State::Sha384(Sha384::new()),
                Algorithm::Sha512 => State::Sha512(Sha512::new()),
                Algorithm::Sha3_256 => State::Sha3(Sha3_256::new()),
                Algorithm::Blake3 => State::Blake3(Blake3Hasher::new()),
                Algorithm::Crc32 => State::Crc32(crc32fast::Hasher::new()),
            };
        };
        match algorithm {
            Algorithm::Md5 => State::HmacMd5(hmac_new(key)),
            Algorithm::Sha1 => State::HmacSha1(hmac_new(key)),
            Algorithm::Sha224 => State::HmacSha224(hmac_new(key)),
            Algorithm::Sha256 => State::HmacSha256(hmac_new(key)),
            Algorithm::Sha384 => State::HmacSha384(hmac_new(key)),
            Algorithm::Sha512 => State::HmacSha512(hmac_new(key)),
            Algorithm::Sha3_256 => State::HmacSha3(hmac_new(key)),
            Algorithm::Blake3 | Algorithm::Crc32 => State::Unsupported,
        }
    }

    fn is_hmac(&self) -> bool {
        matches!(
            self,
            State::HmacMd5(_)
                | State::HmacSha1(_)
                | State::HmacSha224(_)
                | State::HmacSha256(_)
                | State::HmacSha384(_)
                | State::HmacSha512(_)
                | State::HmacSha3(_)
        )
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            State::Md5(hasher) => hasher.update(data),
            State::Sha1(hasher) => hasher.update(data),
            State::Sha224(hasher) => hasher.update(data),
            State::Sha256(hasher) => hasher.update(data),
            State::Sha384(hasher) => hasher.update(data),
            State::Sha512(hasher) => hasher.update(data),
            State::Sha3(hasher) => hasher.update(data),
            State::HmacMd5(hasher) => hasher.update(data),
            State::HmacSha1(hasher) => hasher.update(data),
            State::HmacSha224(hasher) => hasher.update(data),
            State::HmacSha256(hasher) => hasher.update(data),
            State::HmacSha384(hasher) => hasher.update(data),
            State::HmacSha512(hasher) => hasher.update(data),
            State::HmacSha3(hasher) => hasher.update(data),
            State::Blake3(hasher) => {
                hasher.update(data);
            }
            State::Crc32(hasher) => hasher.update(data),
            State::Unsupported => {}
        }
    }

    fn finish(self) -> Vec<u8> {
        match self {
            State::Md5(hasher) => hasher.finalize().to_vec(),
            State::Sha1(hasher) => hasher.finalize().to_vec(),
            State::Sha224(hasher) => hasher.finalize().to_vec(),
            State::Sha256(hasher) => hasher.finalize().to_vec(),
            State::Sha384(hasher) => hasher.finalize().to_vec(),
            State::Sha512(hasher) => hasher.finalize().to_vec(),
            State::Sha3(hasher) => hasher.finalize().to_vec(),
            State::HmacMd5(hasher) => hasher.finalize().into_bytes().to_vec(),
            State::HmacSha1(hasher) => hasher.finalize().into_bytes().to_vec(),
            State::HmacSha224(hasher) => hasher.finalize().into_bytes().to_vec(),
            State::HmacSha256(hasher) => hasher.finalize().into_bytes().to_vec(),
            State::HmacSha384(hasher) => hasher.finalize().into_bytes().to_vec(),
            State::HmacSha512(hasher) => hasher.finalize().into_bytes().to_vec(),
            State::HmacSha3(hasher) => hasher.finalize().into_bytes().to_vec(),
            State::Blake3(hasher) => hasher.finalize().as_bytes().to_vec(),
            State::Crc32(hasher) => hasher.finalize().to_be_bytes().to_vec(),
            State::Unsupported => Vec::new(),
        }
    }
}

fn hmac_new<M: Mac + KeyInit>(key: &[u8]) -> M {
    <M as KeyInit>::new_from_slice(key).expect("HMAC accepts every key length")
}

pub fn matching_labels<'a>(rows: &'a [DigestRow], expected: &str) -> Vec<&'a str> {
    rows.iter()
        .filter(|row| row.matches(expected))
        .map(|row| row.label)
        .collect()
}
