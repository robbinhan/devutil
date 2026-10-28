use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use hash_core::{
    Algorithm, HashError, base64_encode, hash_bytes, hash_path, hash_read, hash_text, hex_encode,
    matching_labels, normalize_expected,
};

fn row<'a>(
    rows: &'a [hash_core::DigestRow],
    algorithm: Algorithm,
) -> &'a hash_core::DigestRow {
    rows.iter()
        .find(|row| row.algorithm == algorithm)
        .expect("row")
}

fn hex(rows: &[hash_core::DigestRow], algorithm: Algorithm) -> String {
    row(rows, algorithm).display(false, false)
}

#[test]
fn empty_and_abc_match_known_digests() {
    let empty = hash_text("", None);
    assert_eq!(empty.len(), 9);
    assert_eq!(
        hex(&empty, Algorithm::Md5),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
    assert_eq!(
        hex(&empty, Algorithm::Sha1),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
    assert_eq!(
        hex(&empty, Algorithm::Sha256),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&empty, Algorithm::Sha3_256),
        "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a"
    );
    assert_eq!(
        hex(&empty, Algorithm::Blake3),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    assert_eq!(hex(&empty, Algorithm::Crc32), "00000000");
    assert_eq!(
        row(&empty, Algorithm::Md5).display(false, true),
        "1B2M2Y8AsgTpgAmY7PhCfg=="
    );

    let abc = hash_text("abc", None);
    assert_eq!(
        hex(&abc, Algorithm::Md5),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        hex(&abc, Algorithm::Sha1),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        hex(&abc, Algorithm::Sha224),
        "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7"
    );
    assert_eq!(
        hex(&abc, Algorithm::Sha256),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&abc, Algorithm::Sha384),
        "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
    );
    assert_eq!(
        hex(&abc, Algorithm::Sha512),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    assert_eq!(
        hex(&abc, Algorithm::Sha3_256),
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );
    assert_eq!(
        hex(&abc, Algorithm::Blake3),
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    assert_eq!(hex(&abc, Algorithm::Crc32), "352441c2");
    assert!(empty.iter().all(|row| row.available));
    assert_eq!(row(&abc, Algorithm::Sha256).display(true, false), hex(&abc, Algorithm::Sha256).to_ascii_uppercase());
}

#[test]
fn crc32_of_the_check_string_is_ieee() {
    let rows = hash_text("123456789", None);
    assert_eq!(hex(&rows, Algorithm::Crc32), "cbf43926");
}

#[test]
fn hmac_covers_md5_sha1_sha256_and_sha512() {
    let data = "The quick brown fox jumps over the lazy dog";
    let rows = hash_text(data, Some(b"key"));
    assert_eq!(hex(&rows, Algorithm::Md5), "80070713463e7749b90c2dc24911e275");
    assert_eq!(hex(&rows, Algorithm::Sha1), "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9");
    assert_eq!(
        hex(&rows, Algorithm::Sha256),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert_eq!(
        hex(&rows, Algorithm::Sha512),
        "b42af09057bac1e2d41708e48a902e09b5ff7f12ab428a4fe86653c73dd248fb82f948a549f7b791a5b41915ee4d1ec3935357e4e2317250d0372afa2ebeeb3a"
    );
    assert_eq!(row(&rows, Algorithm::Sha256).label, "HMAC-SHA-256");
    assert!(!row(&rows, Algorithm::Blake3).available);
    assert!(!row(&rows, Algorithm::Crc32).available);
    assert!(row(&rows, Algorithm::Sha224).available);
    assert!(row(&rows, Algorithm::Sha3_256).available);

    let key = vec![0x0bu8; 20];
    let rfc = hash_bytes(b"Hi There", Some(&key));
    assert_eq!(
        hex(&rfc, Algorithm::Sha256),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
    assert_eq!(
        hex(&rfc, Algorithm::Sha512),
        "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
    );
}

#[test]
fn compare_is_case_insensitive_for_hex_and_base64() {
    let rows = hash_text("abc", None);
    let sha = hex(&rows, Algorithm::Sha256);
    assert_eq!(matching_labels(&rows, &sha), vec!["SHA-256"]);
    assert_eq!(matching_labels(&rows, &sha.to_ascii_uppercase()), vec!["SHA-256"]);
    assert_eq!(
        matching_labels(&rows, &format!("  sha256: {sha}  ")),
        vec!["SHA-256"]
    );
    assert_eq!(matching_labels(&rows, &format!("0x{sha}")), vec!["SHA-256"]);
    let b64 = row(&rows, Algorithm::Md5).display(false, true);
    assert_eq!(matching_labels(&rows, &b64), vec!["MD5"]);
    assert_eq!(matching_labels(&rows, &b64.to_ascii_lowercase()), vec!["MD5"]);
    assert!(matching_labels(&rows, "").is_empty());
    assert!(matching_labels(&rows, "deadbeef").is_empty());
    assert_eq!(normalize_expected("SHA-256:\n abcdabcd"), "abcdabcd");
}

#[test]
fn streamed_chunks_match_one_shot_and_honor_cancel() {
    let mut data = vec![0u8; 50_000];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    let mut seen = Vec::new();
    let cancel = AtomicBool::new(false);
    let streamed = hash_read(io::Cursor::new(data.clone()), data.len() as u64, None, 64, |done, total| {
        seen.push((done, total));
    }, &cancel)
    .unwrap();
    assert_eq!(streamed, hash_bytes(&data, None));
    assert!(seen.len() > 1);
    assert!(seen.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert_eq!(seen.last().copied(), Some((data.len() as u64, data.len() as u64)));

    let cancel = AtomicBool::new(false);
    let error = hash_read(
        io::Cursor::new(data),
        50_000,
        Some(b"key"),
        64,
        |done, _| {
            if done > 0 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert!(matches!(error, HashError::Cancelled));
}

struct CountingReader {
    left: usize,
    reads: Vec<usize>,
}

impl Read for CountingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let n = buf.len().min(self.left);
        buf[..n].fill(7);
        self.left -= n;
        self.reads.push(n);
        Ok(n)
    }
}

#[test]
fn each_read_stays_inside_the_chunk() {
    let mut reader = CountingReader {
        left: 1000,
        reads: Vec::new(),
    };
    let cancel = AtomicBool::new(false);
    let rows = {
        let reader_ref = &mut reader;
        hash_read(reader_ref, 1000, None, 64, |_, _| {}, &cancel).unwrap()
    };
    assert!(reader.reads.len() > 1);
    assert!(reader.reads.iter().all(|n| *n <= 64));
    let bytes: Vec<u8> = std::iter::repeat(7).take(1000).collect();
    assert_eq!(rows, hash_bytes(&bytes, None));
    let _ = hex_encode(&rows[0].bytes, false);
    let _ = base64_encode(&rows[0].bytes);
}

#[test]
fn a_file_larger_than_one_chunk_matches_the_buffer() {
    let path = std::env::temp_dir().join(format!(
        "devutil-hash-{}-{}.bin",
        std::process::id(),
        line!()
    ));
    let mut data = vec![0u8; hash_core::CHUNK_SIZE + 100];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = (index % 251) as u8;
    }
    std::fs::write(&path, &data).unwrap();
    let mut seen = Vec::new();
    let cancel = AtomicBool::new(false);
    let hashed = hash_path(&path, None, |done, total| seen.push((done, total)), &cancel).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(hashed.size, data.len() as u64);
    assert_eq!(hashed.rows, hash_bytes(&data, None));
    assert!(seen.len() >= 2);
    assert_eq!(seen.last().copied(), Some((data.len() as u64, data.len() as u64)));
}
