//! The bounded, deterministic CBOR envelope shared by all backup hosts.
use crate::codec::{canonical, decode};
use crate::crypto::{Key, random_key};
use crate::files::{CHUNK, nonce};
use cose2::{Encrypt0Message, aes_gcm::AesGcmEncryptor, iana::AlgorithmA256GCM};
use dmsg_types::{Error, Result, ensure, ensure_valid};
use icu_normalizer::ComposingNormalizer;
use serde::{Serialize, de::DeserializeOwned};
use serde_bytes::{ByteArray, Bytes};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use unicode_segmentation::UnicodeSegmentation;
use zeroize::{Zeroize, Zeroizing};

pub(super) const MAX_ARCHIVE: usize = 320 << 20;
pub(super) const MAX_RECORD: usize = 256 << 10;
const FORMAT: &str = "dmsg-vault-backup/1";
const WRAP: &str = "dmsg/vault-backup/wrap/1";
const BLOCK: &str = "dmsg/vault-backup/chunk/1";
const COSE_PREFIX: &[u8] = &[0x83, 0x43, 0xa1, 1, 3, 0xa1, 5, 0x4c];
type Header = (String, u8, ByteArray<16>, u64);

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;

pub(super) fn password(value: &str, create: bool) -> Result<Zeroizing<String>> {
    ensure_valid(value.len() <= 1024, "backup password")?;
    let normalized = Zeroizing::new(ComposingNormalizer::new_nfc().normalize(value).into_owned());
    ensure_valid(
        !normalized.is_empty() && normalized.len() <= 256,
        "backup password",
    )?;
    if create {
        ensure_valid(
            normalized.graphemes(true).count() >= 6,
            "backup password length",
        )?;
    }
    Ok(normalized)
}

fn derive(password: &str, salt: &[u8]) -> Result<Zeroizing<Key>> {
    crate::kdf::derive(password.as_bytes(), salt, 65536, 3, 4)
}

fn cipher(key: &Key) -> AesGcmEncryptor {
    AesGcmEncryptor::new(AlgorithmA256GCM, key, None).expect("32-byte AES key")
}

fn seal(cipher: &AesGcmEncryptor, iv: &[u8], bytes: Vec<u8>, aad: Vec<u8>) -> Result<Vec<u8>> {
    let mut message = Encrypt0Message::new(Some(bytes));
    message.unprotected.set_iv(iv);
    let encrypted = message.encrypt(cipher, Some(&aad));
    message.payload.zeroize();
    encrypted.map_err(|_| Error::IntegrityFailed)?;
    message
        .to_untagged_vec()
        .map_err(|_| Error::IntegrityFailed)
}

fn open(
    cipher: &AesGcmEncryptor,
    bytes: &[u8],
    iv: Option<&[u8]>,
    aad: Vec<u8>,
) -> Result<Zeroizing<Vec<u8>>> {
    ensure(bytes.starts_with(COSE_PREFIX), Error::IntegrityFailed)?;
    let mut message = Encrypt0Message::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        message
            .to_untagged_vec()
            .map_err(|_| Error::IntegrityFailed)?
            == bytes,
        Error::IntegrityFailed,
    )?;
    let actual = message
        .unprotected
        .iv()
        .map_err(|_| Error::IntegrityFailed)?;
    ensure(iv.is_none() || iv == actual, Error::IntegrityFailed)?;
    message
        .decrypt(cipher, Some(&aad))
        .map_err(|_| Error::IntegrityFailed)?;
    Ok(Zeroizing::new(
        message.payload.take().ok_or(Error::IntegrityFailed)?,
    ))
}

pub(super) fn file_io(error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData => Error::IntegrityFailed,
        _ => Error::Unavailable(format!("backup file: {}", error.kind())),
    }
}

/// A shortest, definite CBOR length prefix.
pub(super) fn prefix(major: u8, value: u64) -> Vec<u8> {
    let mut bytes = canonical(&value);
    bytes[0] |= major << 5;
    bytes
}

fn head(input: &mut impl Read) -> Result<(u8, u64, Vec<u8>)> {
    let mut first = [0];
    input.read_exact(&mut first).map_err(file_io)?;
    let major = first[0] >> 5;
    let info = first[0] & 31;
    let value = match info {
        0..=23 => u64::from(info),
        24..=27 => {
            let n = 1 << (info - 24);
            let mut raw = [0; 8];
            input.read_exact(&mut raw[8 - n..]).map_err(file_io)?;
            u64::from_be_bytes(raw)
        }
        _ => return Err(Error::IntegrityFailed),
    };
    let encoded = prefix(major, value);
    ensure(encoded[0] == first[0], Error::IntegrityFailed)?;
    Ok((major, value, encoded))
}

pub(super) fn length(input: &mut impl Read, major: u8, max: usize) -> Result<usize> {
    let (kind, size, _) = head(input)?;
    ensure(kind == major, Error::IntegrityFailed)?;
    ensure(size <= max as u64, Error::QuotaExceeded)?;
    Ok(size as usize)
}

fn bytes(input: &mut impl Read, max: usize) -> Result<Vec<u8>> {
    let size = length(input, 2, max)?;
    let mut buf = vec![0; size];
    input.read_exact(&mut buf).map_err(file_io)?;
    Ok(buf)
}

// Read precisely one small metadata record. Reject declared string lengths before
// allocating; the normal workspace decoder then validates its canonical structure.
pub(super) fn record<T: DeserializeOwned + Serialize>(
    input: &mut impl Read,
    max: usize,
) -> Result<(T, usize)> {
    fn take(input: &mut impl Read, out: &mut Vec<u8>, depth: usize, max: usize) -> Result<()> {
        ensure(depth <= 24 && out.len() < max, Error::QuotaExceeded)?;
        let (kind, n, initial) = head(input)?;
        out.extend(initial);
        ensure(out.len() <= max, Error::QuotaExceeded)?;
        match kind {
            0 | 1 | 7 => {}
            2 | 3 => {
                ensure(n <= (max - out.len()) as u64, Error::QuotaExceeded)?;
                let start = out.len();
                out.resize(start + n as usize, 0);
                input.read_exact(&mut out[start..]).map_err(file_io)?;
            }
            4 | 5 => {
                let count = n
                    .checked_mul(if kind == 5 { 2 } else { 1 })
                    .ok_or(Error::QuotaExceeded)?;
                ensure(count <= (max - out.len()) as u64, Error::QuotaExceeded)?;
                for _ in 0..count {
                    take(input, out, depth + 1, max)?;
                }
            }
            _ => return Err(Error::IntegrityFailed),
        }
        Ok(())
    }
    let mut raw = Zeroizing::new(Vec::new());
    take(input, &mut raw, 0, max)?;
    let value = decode(&raw, max)?;
    Ok((value, raw.len()))
}

pub(super) struct Writer<W> {
    out: W,
    cipher: AesGcmEncryptor,
    h: Header,
    index: u64,
    written: u64,
    buf: Zeroizing<Vec<u8>>,
}

impl<W: Write> Writer<W> {
    pub(super) fn new(out: W, size: usize, password: &str) -> Result<Self> {
        let random = random_key();
        Self::with_keys(
            out,
            size,
            password,
            random[..16].try_into().unwrap(),
            &random[16..28],
            &Zeroizing::new(random_key()),
        )
    }

    fn with_keys(
        mut out: W,
        size: usize,
        password: &str,
        salt: [u8; 16],
        iv: &[u8],
        key: &Key,
    ) -> Result<Self> {
        ensure((1..=MAX_ARCHIVE).contains(&size), Error::QuotaExceeded)?;
        let h: Header = (FORMAT.into(), 1, salt.into(), size as u64);
        let wrap = cipher(&*derive(password, &salt)?);
        let key_box = seal(&wrap, iv, key.to_vec(), canonical(&(WRAP, &h)))?;
        out.write_all(&[0x83])
            .and_then(|_| out.write_all(&canonical(&h)))
            .and_then(|_| out.write_all(&canonical(&Bytes::new(&key_box))))
            .and_then(|_| out.write_all(&prefix(4, size.div_ceil(CHUNK) as u64)))
            .map_err(file_io)?;
        Ok(Self {
            out,
            cipher: cipher(key),
            h,
            index: 0,
            written: 0,
            buf: Zeroizing::new(Vec::with_capacity(CHUNK)),
        })
    }

    fn flush_block(&mut self) -> Result<()> {
        let block = seal(
            &self.cipher,
            &nonce(self.index),
            std::mem::take(&mut *self.buf),
            canonical(&(BLOCK, &self.h, self.index)),
        )?;
        self.out
            .write_all(&prefix(2, block.len() as u64))
            .and_then(|_| self.out.write_all(&block))
            .map_err(file_io)?;
        self.index += 1;
        Ok(())
    }

    pub(super) fn push(&mut self, mut bytes: &[u8]) -> Result<()> {
        self.written += bytes.len() as u64;
        ensure(self.written <= self.h.3, Error::IntegrityFailed)?;
        while !bytes.is_empty() {
            let (part, rest) = bytes.split_at(bytes.len().min(CHUNK - self.buf.len()));
            self.buf.extend_from_slice(part);
            if self.buf.len() == CHUNK {
                self.flush_block()?;
            }
            bytes = rest;
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<W> {
        ensure(self.written == self.h.3, Error::IntegrityFailed)?;
        if !self.buf.is_empty() {
            self.flush_block()?;
        }
        self.out.flush().map_err(file_io)?;
        Ok(self.out)
    }
}

struct Hashed<R> {
    source: R,
    digest: Sha256,
}

impl<R: Read> Read for Hashed<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.source.read(buf)?;
        self.digest.update(&buf[..count]);
        Ok(count)
    }
}

pub(super) struct Reader<R> {
    input: Hashed<R>,
    cipher: AesGcmEncryptor,
    h: Header,
    buf: Zeroizing<Vec<u8>>,
    offset: usize,
    index: u64,
}

impl<R: Read> Reader<R> {
    pub(super) fn new(input: R, password: &str) -> Result<Self> {
        let mut input = Hashed {
            source: input,
            digest: Sha256::new(),
        };
        ensure(length(&mut input, 4, 3)? == 3, Error::IntegrityFailed)?;
        let (h, _): (Header, _) = record(&mut input, 128)?;
        ensure(h.0 == FORMAT && h.1 == 1, Error::UnsupportedProtocol)?;
        ensure(
            (1..=MAX_ARCHIVE as u64).contains(&h.3),
            Error::QuotaExceeded,
        )?;
        let wrapped = bytes(&mut input, 128)?;
        let count = length(&mut input, 4, MAX_ARCHIVE / CHUNK)?;
        ensure(
            count as u64 == h.3.div_ceil(CHUNK as u64),
            Error::IntegrityFailed,
        )?;
        let wrap = cipher(&*derive(password, h.2.as_ref())?);
        let key = open(&wrap, &wrapped, None, canonical(&(WRAP, &h)))?;
        let cipher = cipher(
            key.as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        );
        Ok(Self {
            input,
            cipher,
            h,
            buf: Zeroizing::new(Vec::new()),
            offset: 0,
            index: 0,
        })
    }

    fn next_block(&mut self) -> Result<()> {
        let sealed = bytes(&mut self.input, CHUNK + 64)?;
        let block = open(
            &self.cipher,
            &sealed,
            Some(&nonce(self.index)),
            canonical(&(BLOCK, &self.h, self.index)),
        )?;
        let remaining = self.h.3 - self.index * CHUNK as u64;
        ensure(
            block.len() as u64 == remaining.min(CHUNK as u64),
            Error::IntegrityFailed,
        )?;
        self.buf = block;
        self.offset = 0;
        self.index += 1;
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<String> {
        ensure(
            self.offset == self.buf.len() && self.index == self.h.3.div_ceil(CHUNK as u64),
            Error::IntegrityFailed,
        )?;
        ensure(
            self.input.read(&mut [0]).map_err(file_io)? == 0,
            Error::IntegrityFailed,
        )?;
        Ok(hex::encode(self.input.digest.finalize()))
    }
}

impl<R: Read> Read for Reader<R> {
    fn read(&mut self, dest: &mut [u8]) -> io::Result<usize> {
        if dest.is_empty() {
            return Ok(0);
        }
        if self.offset == self.buf.len() {
            if self.index == self.h.3.div_ceil(CHUNK as u64) {
                return Ok(0);
            }
            self.next_block().map_err(|_| io::ErrorKind::InvalidData)?;
        }
        let count = dest.len().min(self.buf.len() - self.offset);
        dest[..count].copy_from_slice(&self.buf[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}
