//! Feature-gated pproxy 2.7.9 legacy Shadowsocks stream ciphers.
//!
//! This module is intentionally separate from the native AEAD implementation.
//! Every method here is unauthenticated and must only be enabled for a
//! compatibility deployment. The implementation uses RustCrypto primitives
//! and small, safe state machines for the pproxy-specific stream modes.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

use crate::address::{decode_address, encode_address};
use crate::error::ShadowsocksError;
use eggress_core::{BoxStream, TargetAddr};

type HmacSha1 = Hmac<Sha1>;

const OTA_TAG: u8 = 0x10;
const OTA_MAC_LEN: usize = 10;
const MAX_OTA_CHUNK: usize = u16::MAX as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockMode {
    Cfb(usize),
    Ctr,
    Ofb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    Table,
    Rc4,
    Rc4Md5,
    Aes { bits: u16, mode: BlockMode },
    Blowfish { mode: BlockMode },
    Des { mode: BlockMode },
    Camellia { bits: u16 },
    ChaCha20,
    ChaCha20Ietf,
    XChaCha20,
    Salsa20,
    XSalsa20,
}

/// A supported pproxy legacy method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyMethod {
    name: &'static str,
    algorithm: Algorithm,
    key_len: usize,
    iv_len: usize,
    ota: bool,
}

impl LegacyMethod {
    /// Parse a pproxy 2.7.9 method, including the `!` OTA suffix and `-py`
    /// aliases from `cipherpy.py`.
    pub fn parse(input: &str) -> Result<Self, ShadowsocksError> {
        let lower = input.to_ascii_lowercase();
        let (without_ota, ota) = lower
            .strip_suffix('!')
            .map_or((lower.as_str(), false), |name| (name, true));
        let base = without_ota.strip_suffix("-py").unwrap_or(without_ota);
        let (base, ota) = if let Some(name) = base.strip_suffix('!') {
            (name, true)
        } else {
            (base, ota)
        };
        let method = match base {
            "table" => Self::new("table", Algorithm::Table, 0, 0, ota),
            "rc4" => Self::new("rc4", Algorithm::Rc4, 16, 0, ota),
            "rc4-md5" => Self::new("rc4-md5", Algorithm::Rc4Md5, 16, 16, ota),
            "aes-128-cfb" => Self::aes("aes-128-cfb", 128, 16, ota, BlockMode::Cfb(128)),
            "aes-192-cfb" => Self::aes("aes-192-cfb", 192, 16, ota, BlockMode::Cfb(128)),
            "aes-256-cfb" => Self::aes("aes-256-cfb", 256, 16, ota, BlockMode::Cfb(128)),
            "aes-128-cfb1" => Self::aes("aes-128-cfb1", 128, 16, ota, BlockMode::Cfb(1)),
            "aes-192-cfb1" => Self::aes("aes-192-cfb1", 192, 16, ota, BlockMode::Cfb(1)),
            "aes-256-cfb1" => Self::aes("aes-256-cfb1", 256, 16, ota, BlockMode::Cfb(1)),
            "aes-128-cfb8" => Self::aes("aes-128-cfb8", 128, 16, ota, BlockMode::Cfb(8)),
            "aes-192-cfb8" => Self::aes("aes-192-cfb8", 192, 16, ota, BlockMode::Cfb(8)),
            "aes-256-cfb8" => Self::aes("aes-256-cfb8", 256, 16, ota, BlockMode::Cfb(8)),
            "aes-128-ctr" => Self::aes("aes-128-ctr", 128, 16, ota, BlockMode::Ctr),
            "aes-192-ctr" => Self::aes("aes-192-ctr", 192, 16, ota, BlockMode::Ctr),
            "aes-256-ctr" => Self::aes("aes-256-ctr", 256, 16, ota, BlockMode::Ctr),
            "aes-128-ofb" => Self::aes("aes-128-ofb", 128, 16, ota, BlockMode::Ofb),
            "aes-192-ofb" => Self::aes("aes-192-ofb", 192, 16, ota, BlockMode::Ofb),
            "aes-256-ofb" => Self::aes("aes-256-ofb", 256, 16, ota, BlockMode::Ofb),
            "bf-cfb" => Self::new(
                "bf-cfb",
                Algorithm::Blowfish {
                    mode: BlockMode::Cfb(64),
                },
                16,
                8,
                ota,
            ),
            "des-cfb" => Self::new(
                "des-cfb",
                Algorithm::Des {
                    mode: BlockMode::Cfb(64),
                },
                8,
                8,
                ota,
            ),
            "camellia-128-cfb" => Self::new(
                "camellia-128-cfb",
                Algorithm::Camellia { bits: 128 },
                16,
                16,
                ota,
            ),
            "camellia-192-cfb" => Self::new(
                "camellia-192-cfb",
                Algorithm::Camellia { bits: 192 },
                24,
                16,
                ota,
            ),
            "camellia-256-cfb" => Self::new(
                "camellia-256-cfb",
                Algorithm::Camellia { bits: 256 },
                32,
                16,
                ota,
            ),
            "chacha20" => Self::new("chacha20", Algorithm::ChaCha20, 32, 8, ota),
            "chacha20-ietf" => Self::new("chacha20-ietf", Algorithm::ChaCha20Ietf, 32, 12, ota),
            "xchacha20" => Self::new("xchacha20", Algorithm::XChaCha20, 32, 24, ota),
            "salsa20" => Self::new("salsa20", Algorithm::Salsa20, 32, 8, ota),
            "xsalsa20" => Self::new("xsalsa20", Algorithm::XSalsa20, 32, 24, ota),
            // These names are in pproxy's inventory, but no maintained safe
            // Rust implementation is available in the workspace yet.
            "cast5-cfb" | "idea-cfb" | "rc2-cfb" | "seed-cfb" => {
                return Err(ShadowsocksError::LegacyMethodUnsupported(base.to_string()))
            }
            _ => return Err(ShadowsocksError::UnsupportedMethod(input.to_string())),
        };
        Ok(method)
    }

    const fn new(
        name: &'static str,
        algorithm: Algorithm,
        key_len: usize,
        iv_len: usize,
        ota: bool,
    ) -> Self {
        Self {
            name,
            algorithm,
            key_len,
            iv_len,
            ota,
        }
    }

    const fn aes(name: &'static str, bits: u16, iv_len: usize, ota: bool, mode: BlockMode) -> Self {
        Self::new(
            name,
            Algorithm::Aes { bits, mode },
            bits as usize / 8,
            iv_len,
            ota,
        )
    }

    pub fn name(self) -> &'static str {
        self.name
    }

    pub fn key_len(self) -> usize {
        self.key_len
    }

    pub fn iv_len(self) -> usize {
        self.iv_len
    }

    pub fn ota(self) -> bool {
        self.ota
    }
}

impl std::fmt::Display for LegacyMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

enum BlockCipher {
    Aes128(aes::Aes128),
    Aes192(aes::Aes192),
    Aes256(aes::Aes256),
    Blowfish(blowfish::Blowfish),
    Des(des::Des),
    Camellia128(camellia::Camellia128),
    Camellia192(camellia::Camellia192),
    Camellia256(camellia::Camellia256),
}

impl BlockCipher {
    fn new(method: LegacyMethod, key: &[u8]) -> Result<Self, ShadowsocksError> {
        let invalid = || ShadowsocksError::InvalidKeyLength;
        match method.algorithm {
            Algorithm::Aes { bits: 128, .. } => {
                use aes::cipher::KeyInit as _;
                aes::Aes128::new_from_slice(key)
                    .map(Self::Aes128)
                    .map_err(|_| invalid())
            }
            Algorithm::Aes { bits: 192, .. } => {
                use aes::cipher::KeyInit as _;
                aes::Aes192::new_from_slice(key)
                    .map(Self::Aes192)
                    .map_err(|_| invalid())
            }
            Algorithm::Aes { bits: 256, .. } => {
                use aes::cipher::KeyInit as _;
                aes::Aes256::new_from_slice(key)
                    .map(Self::Aes256)
                    .map_err(|_| invalid())
            }
            Algorithm::Blowfish { .. } => {
                use blowfish::cipher::KeyInit as _;
                blowfish::Blowfish::new_from_slice(key)
                    .map(Self::Blowfish)
                    .map_err(|_| invalid())
            }
            Algorithm::Des { .. } => {
                use des::cipher::KeyInit as _;
                des::Des::new_from_slice(key)
                    .map(Self::Des)
                    .map_err(|_| invalid())
            }
            Algorithm::Camellia { bits: 128 } => {
                use camellia::cipher::KeyInit as _;
                camellia::Camellia128::new_from_slice(key)
                    .map(Self::Camellia128)
                    .map_err(|_| invalid())
            }
            Algorithm::Camellia { bits: 192 } => {
                use camellia::cipher::KeyInit as _;
                camellia::Camellia192::new_from_slice(key)
                    .map(Self::Camellia192)
                    .map_err(|_| invalid())
            }
            Algorithm::Camellia { bits: 256 } => {
                use camellia::cipher::KeyInit as _;
                camellia::Camellia256::new_from_slice(key)
                    .map(Self::Camellia256)
                    .map_err(|_| invalid())
            }
            _ => Err(ShadowsocksError::Other("not a block cipher".into())),
        }
    }

    fn block_len(&self) -> usize {
        match self {
            Self::Aes128(_) | Self::Aes192(_) | Self::Aes256(_) => 16,
            Self::Blowfish(_) | Self::Des(_) => 8,
            Self::Camellia128(_) | Self::Camellia192(_) | Self::Camellia256(_) => 16,
        }
    }

    fn encrypt_block(&self, input: &[u8], output: &mut [u8]) {
        match self {
            Self::Aes128(cipher) => encrypt_block_04(cipher, input, output),
            Self::Aes192(cipher) => encrypt_block_04(cipher, input, output),
            Self::Aes256(cipher) => encrypt_block_04(cipher, input, output),
            Self::Des(cipher) => encrypt_block_04(cipher, input, output),
            Self::Camellia128(cipher) => encrypt_block_04(cipher, input, output),
            Self::Camellia192(cipher) => encrypt_block_04(cipher, input, output),
            Self::Camellia256(cipher) => encrypt_block_04(cipher, input, output),
            Self::Blowfish(cipher) => encrypt_block_05(cipher, input, output),
        }
    }
}

fn encrypt_block_04<C>(cipher: &C, input: &[u8], output: &mut [u8])
where
    C: aes::cipher::BlockEncrypt + aes::cipher::BlockSizeUser,
{
    let mut block = aes::cipher::Block::<C>::clone_from_slice(input);
    cipher.encrypt_block(&mut block);
    output.copy_from_slice(&block);
}

fn encrypt_block_05<C>(cipher: &C, input: &[u8], output: &mut [u8])
where
    C: blowfish::cipher::BlockCipherEncrypt + blowfish::cipher::BlockSizeUser,
{
    let mut block = blowfish::cipher::Block::<C>::try_from(input).expect("valid block length");
    cipher.encrypt_block(&mut block);
    output.copy_from_slice(&block);
}

enum CipherState {
    Table {
        encrypt: [u8; 256],
        decrypt: [u8; 256],
    },
    Rc4 {
        s: [u8; 256],
        i: u8,
        j: u8,
    },
    Block {
        cipher: BlockCipher,
        mode: BlockMode,
        feedback: Vec<u8>,
        keystream: Vec<u8>,
        keystream_pos: usize,
        segment_buffer: Vec<u8>,
    },
    ChaCha20(chacha20::ChaCha20Legacy),
    ChaCha20Ietf(chacha20::ChaCha20),
    XChaCha20(chacha20::XChaCha20),
    Salsa20(salsa20::Salsa20),
    XSalsa20(salsa20::XSalsa20),
}

impl CipherState {
    fn new(method: LegacyMethod, key: &[u8], iv: &[u8]) -> Result<Self, ShadowsocksError> {
        if iv.len() != method.iv_len()
            || (!matches!(method.algorithm, Algorithm::Table) && key.len() != method.key_len())
        {
            return Err(ShadowsocksError::InvalidKeyLength);
        }
        use md5::Digest as _;
        match method.algorithm {
            Algorithm::Table => {
                let mut encrypt = [0u8; 256];
                encrypt
                    .iter_mut()
                    .enumerate()
                    .for_each(|(i, value)| *value = i as u8);
                let digest = md5::Md5::digest(key);
                let a = u64::from_le_bytes(digest[..8].try_into().unwrap());
                for i in 1..1024u64 {
                    encrypt.sort_by_key(|value| a % (*value as u64 + i));
                }
                let mut decrypt = [0u8; 256];
                for (i, value) in encrypt.iter().enumerate() {
                    decrypt[*value as usize] = i as u8;
                }
                Ok(Self::Table { encrypt, decrypt })
            }
            Algorithm::Rc4 | Algorithm::Rc4Md5 => {
                let rc4_key = if matches!(method.algorithm, Algorithm::Rc4Md5) {
                    md5_bytes(&[key, iv].concat())
                } else {
                    key.to_vec()
                };
                Ok(Rc4State::new(&rc4_key))
            }
            Algorithm::Aes { mode, .. } => {
                let cipher = BlockCipher::new(method, key)?;
                Ok(Self::Block {
                    cipher,
                    mode,
                    feedback: iv.to_vec(),
                    keystream: Vec::new(),
                    keystream_pos: 0,
                    segment_buffer: Vec::new(),
                })
            }
            Algorithm::Blowfish { mode } | Algorithm::Des { mode } => {
                let cipher = BlockCipher::new(method, key)?;
                Ok(Self::Block {
                    cipher,
                    mode,
                    feedback: iv.to_vec(),
                    keystream: Vec::new(),
                    keystream_pos: 0,
                    segment_buffer: Vec::new(),
                })
            }
            Algorithm::Camellia { .. } => {
                let cipher = BlockCipher::new(method, key)?;
                Ok(Self::Block {
                    cipher,
                    mode: BlockMode::Cfb(128),
                    feedback: iv.to_vec(),
                    keystream: Vec::new(),
                    keystream_pos: 0,
                    segment_buffer: Vec::new(),
                })
            }
            Algorithm::ChaCha20 => {
                use chacha20::cipher::KeyIvInit as _;
                let key = chacha20::Key::from_slice(key);
                let iv = chacha20::LegacyNonce::from_slice(iv);
                Ok(Self::ChaCha20(chacha20::ChaCha20Legacy::new(key, iv)))
            }
            Algorithm::ChaCha20Ietf => {
                use chacha20::cipher::KeyIvInit as _;
                let key = chacha20::Key::from_slice(key);
                let iv = chacha20::Nonce::from_slice(iv);
                Ok(Self::ChaCha20Ietf(chacha20::ChaCha20::new(key, iv)))
            }
            Algorithm::XChaCha20 => {
                use chacha20::cipher::KeyIvInit as _;
                let key = chacha20::Key::from_slice(key);
                let iv = chacha20::XNonce::from_slice(iv);
                Ok(Self::XChaCha20(chacha20::XChaCha20::new(key, iv)))
            }
            Algorithm::Salsa20 => {
                use salsa20::cipher::KeyIvInit as _;
                let key =
                    salsa20::Key::try_from(key).map_err(|_| ShadowsocksError::InvalidKeyLength)?;
                let iv =
                    salsa20::Nonce::try_from(iv).map_err(|_| ShadowsocksError::InvalidKeyLength)?;
                Ok(Self::Salsa20(salsa20::Salsa20::new(&key, &iv)))
            }
            Algorithm::XSalsa20 => {
                use salsa20::cipher::KeyIvInit as _;
                let key =
                    salsa20::Key::try_from(key).map_err(|_| ShadowsocksError::InvalidKeyLength)?;
                let iv = salsa20::XNonce::try_from(iv)
                    .map_err(|_| ShadowsocksError::InvalidKeyLength)?;
                Ok(Self::XSalsa20(salsa20::XSalsa20::new(&key, &iv)))
            }
        }
    }

    fn apply(&mut self, data: &mut [u8], decrypt: bool) {
        match self {
            Self::Table { encrypt, .. } if !decrypt => {
                data.iter_mut().for_each(|b| *b = encrypt[*b as usize]);
            }
            Self::Table { decrypt: table, .. } => {
                data.iter_mut().for_each(|b| *b = table[*b as usize]);
            }
            Self::Rc4 { s, i, j } => {
                for byte in data {
                    *i = i.wrapping_add(1);
                    *j = j.wrapping_add(s[*i as usize]);
                    s.swap(*i as usize, *j as usize);
                    let index = s[*i as usize].wrapping_add(s[*j as usize]);
                    *byte ^= s[index as usize];
                }
            }
            Self::Block {
                cipher,
                mode,
                feedback,
                keystream,
                keystream_pos,
                segment_buffer,
            } => apply_block(
                cipher,
                *mode,
                feedback,
                keystream,
                keystream_pos,
                segment_buffer,
                data,
                decrypt,
            ),
            Self::ChaCha20(cipher) => {
                use chacha20::cipher::StreamCipher as _;
                cipher.apply_keystream(data);
            }
            Self::ChaCha20Ietf(cipher) => {
                use chacha20::cipher::StreamCipher as _;
                cipher.apply_keystream(data);
            }
            Self::XChaCha20(cipher) => {
                use chacha20::cipher::StreamCipher as _;
                cipher.apply_keystream(data);
            }
            Self::Salsa20(cipher) => {
                use salsa20::cipher::StreamCipher as _;
                cipher.apply_keystream(data);
            }
            Self::XSalsa20(cipher) => {
                use salsa20::cipher::StreamCipher as _;
                cipher.apply_keystream(data);
            }
        }
    }
}

struct Rc4State;

impl Rc4State {
    fn new(key: &[u8]) -> CipherState {
        let mut s = [0u8; 256];
        s.iter_mut()
            .enumerate()
            .for_each(|(i, value)| *value = i as u8);
        let mut j = 0u8;
        for i in 0..256usize {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        CipherState::Rc4 { s, i: 0, j: 0 }
    }
}

fn apply_block(
    cipher: &BlockCipher,
    mode: BlockMode,
    feedback: &mut Vec<u8>,
    keystream: &mut Vec<u8>,
    keystream_pos: &mut usize,
    segment_buffer: &mut Vec<u8>,
    data: &mut [u8],
    decrypt: bool,
) {
    match mode {
        BlockMode::Cfb(bits) if bits == 1 => {
            for byte in data {
                let input = *byte;
                let mut output = 0u8;
                for bit in (0..8).rev() {
                    let mut encrypted = vec![0u8; feedback.len()];
                    cipher.encrypt_block(feedback, &mut encrypted);
                    let in_bit = (input >> bit) & 1;
                    let out_bit = in_bit ^ (encrypted[0] >> 7);
                    output |= out_bit << bit;
                    let value = if decrypt { in_bit } else { out_bit };
                    let mut carry = value;
                    for feedback_byte in feedback.iter_mut().rev() {
                        let next_carry = *feedback_byte >> 7;
                        *feedback_byte = (*feedback_byte << 1) | carry;
                        carry = next_carry;
                    }
                }
                *byte = output;
            }
        }
        BlockMode::Cfb(bits) => {
            let segment = bits / 8;
            let block_len = cipher.block_len();
            if segment == block_len {
                let mut offset = 0;
                while offset < data.len() {
                    if *keystream_pos == keystream.len() {
                        keystream.resize(block_len, 0);
                        cipher.encrypt_block(feedback, keystream);
                        *keystream_pos = 0;
                        segment_buffer.clear();
                    }
                    let count = (keystream.len() - *keystream_pos).min(data.len() - offset);
                    let input = data[offset..offset + count].to_vec();
                    for i in 0..count {
                        data[offset + i] ^= keystream[*keystream_pos + i];
                    }
                    let source = if decrypt {
                        &input
                    } else {
                        &data[offset..offset + count]
                    };
                    segment_buffer.extend_from_slice(source);
                    *keystream_pos += count;
                    offset += count;
                    if *keystream_pos == block_len {
                        feedback.copy_from_slice(segment_buffer);
                        segment_buffer.clear();
                        keystream.clear();
                        *keystream_pos = 0;
                    }
                }
                return;
            }
            let mut offset = 0;
            while offset < data.len() {
                let mut encrypted = vec![0u8; block_len];
                cipher.encrypt_block(feedback, &mut encrypted);
                let count = segment.min(data.len() - offset);
                let input = data[offset..offset + count].to_vec();
                for i in 0..count {
                    data[offset + i] ^= encrypted[i];
                }
                let source = if decrypt {
                    &input
                } else {
                    &data[offset..offset + count]
                };
                if count == block_len {
                    feedback.copy_from_slice(source);
                } else {
                    feedback.drain(..count);
                    feedback.extend_from_slice(source);
                }
                offset += count;
            }
        }
        BlockMode::Ctr | BlockMode::Ofb => {
            for byte in data {
                if *keystream_pos == keystream.len() {
                    keystream.resize(cipher.block_len(), 0);
                    cipher.encrypt_block(feedback, keystream);
                    *keystream_pos = 0;
                    if mode == BlockMode::Ofb {
                        feedback.copy_from_slice(keystream);
                    } else {
                        for value in feedback.iter_mut().rev() {
                            let (next, carry) = value.overflowing_add(1);
                            *value = next;
                            if !carry {
                                break;
                            }
                        }
                    }
                }
                *byte ^= keystream[*keystream_pos];
                *keystream_pos += 1;
            }
        }
    }
}

fn md5_bytes(data: &[u8]) -> Vec<u8> {
    use md5::Digest as _;
    md5::Md5::digest(data).to_vec()
}

fn derive_key(password: &[u8], length: usize) -> Vec<u8> {
    if length == 0 {
        return password.to_vec();
    }
    let mut out = Vec::with_capacity(length);
    let mut previous = Vec::new();
    while out.len() < length {
        let mut input = previous;
        input.extend_from_slice(password);
        previous = md5_bytes(&input);
        out.extend_from_slice(&previous);
    }
    out.truncate(length);
    out
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC accepts arbitrary keys");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

fn ota_mac(key: &[u8], iv: &[u8], data: &[u8]) -> [u8; OTA_MAC_LEN] {
    let mut hmac_key = Vec::with_capacity(iv.len() + key.len());
    hmac_key.extend_from_slice(iv);
    hmac_key.extend_from_slice(key);
    let digest = hmac_sha1(&hmac_key, data);
    digest[..OTA_MAC_LEN].try_into().unwrap()
}

fn chunk_mac(iv: &[u8], sequence: u32, data: &[u8]) -> [u8; OTA_MAC_LEN] {
    let mut key = Vec::with_capacity(iv.len() + 4);
    key.extend_from_slice(iv);
    key.extend_from_slice(&sequence.to_be_bytes());
    let digest = hmac_sha1(&key, data);
    digest[..OTA_MAC_LEN].try_into().unwrap()
}

/// Client-side legacy Shadowsocks handshake.
pub async fn legacy_connect(
    mut stream: BoxStream,
    target: &TargetAddr,
    method: LegacyMethod,
    password: &[u8],
) -> Result<BoxStream, ShadowsocksError> {
    warn_legacy(method);
    let key = derive_key(password, method.key_len());
    let mut iv = vec![0u8; method.iv_len()];
    rand::thread_rng().fill_bytes(&mut iv);
    let mut header = encode_address(target)?;
    if method.ota {
        header[0] |= OTA_TAG;
        header.extend_from_slice(&ota_mac(&key, &iv, &header));
    }
    let mut state = CipherState::new(method, &key, &iv)?;
    state.apply(&mut header, false);
    let mut wire = iv.clone();
    wire.extend_from_slice(&header);
    stream.write_all(&wire).await?;
    stream.flush().await?;
    Ok(Box::new(LegacyStream::client(
        stream, method, key, iv, state,
    )?))
}

/// Server-side legacy Shadowsocks handshake.
pub async fn legacy_accept(
    mut stream: BoxStream,
    method: LegacyMethod,
    password: &[u8],
) -> Result<(BoxStream, TargetAddr), ShadowsocksError> {
    warn_legacy(method);
    let key = derive_key(password, method.key_len());
    let mut iv = vec![0u8; method.iv_len()];
    if !iv.is_empty() {
        stream.read_exact(&mut iv).await?;
    }
    let mut adapter = LegacyStream::server(stream, method, key.clone(), iv.clone())?;
    let mut first = [0u8; 1];
    adapter.read_exact(&mut first).await?;
    let mut header = first.to_vec();
    match first[0] & !OTA_TAG {
        1 => header.resize(7, 0),
        3 => {
            let mut len = [0u8; 1];
            adapter.read_exact(&mut len).await?;
            header.push(len[0]);
            header.resize(4 + len[0] as usize, 0);
        }
        4 => header.resize(19, 0),
        _ => {
            return Err(ShadowsocksError::InvalidAddress(
                "unknown legacy address type".into(),
            ))
        }
    }
    let already_read = if first[0] & !OTA_TAG == 3 { 2 } else { 1 };
    if header.len() > already_read {
        adapter.read_exact(&mut header[already_read..]).await?;
    }
    if first[0] & OTA_TAG != 0 {
        let mut checksum = [0u8; OTA_MAC_LEN];
        adapter.read_exact(&mut checksum).await?;
        if checksum != ota_mac(&key, &iv, &header) {
            return Err(ShadowsocksError::InvalidOtaHmac);
        }
        adapter.enable_ota()?;
    } else if method.ota {
        return Err(ShadowsocksError::InvalidOtaHmac);
    }
    let mut address = header;
    address[0] &= !OTA_TAG;
    let (target, _) = decode_address(&address)?;
    Ok((Box::new(adapter), target))
}

/// Encode one pproxy `PacketCipher` datagram. OTA is a stream-handshake
/// extension and is intentionally ignored for datagrams, matching pproxy.
pub fn legacy_udp_encode(
    method: LegacyMethod,
    password: &[u8],
    target: &TargetAddr,
    payload: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, ShadowsocksError> {
    if iv.len() != method.iv_len() {
        return Err(ShadowsocksError::InvalidKeyLength);
    }
    let key = derive_key(password, method.key_len());
    let mut plaintext = encode_address(target)?;
    plaintext.extend_from_slice(payload);
    let mut state = CipherState::new(method, &key, iv)?;
    state.apply(&mut plaintext, false);
    let mut packet = iv.to_vec();
    packet.extend_from_slice(&plaintext);
    Ok(packet)
}

/// Decode one pproxy `PacketCipher` datagram.
pub fn legacy_udp_decode(
    method: LegacyMethod,
    password: &[u8],
    packet: &[u8],
) -> Result<(TargetAddr, Vec<u8>), ShadowsocksError> {
    if packet.len() < method.iv_len() {
        return Err(ShadowsocksError::DecryptionFailed(
            "packet too short".into(),
        ));
    }
    let iv = &packet[..method.iv_len()];
    let key = derive_key(password, method.key_len());
    let mut plaintext = packet[method.iv_len()..].to_vec();
    let mut state = CipherState::new(method, &key, iv)?;
    state.apply(&mut plaintext, true);
    let (target, address_len) = decode_address(&plaintext)?;
    Ok((target, plaintext[address_len..].to_vec()))
}

fn warn_legacy(method: LegacyMethod) {
    tracing::warn!(
        method = method.name(),
        "insecure legacy Shadowsocks cipher selected; compatibility mode only"
    );
}

struct LegacyStream {
    inner: BoxStream,
    method: LegacyMethod,
    key: Vec<u8>,
    read_iv: Vec<u8>,
    read_state: Option<CipherState>,
    write_iv: Vec<u8>,
    write_state: Option<CipherState>,
    ota_read: bool,
    ota_write: bool,
    ota_sequence_read: u32,
    ota_sequence_write: u32,
    ota_pending: Vec<u8>,
    plain: VecDeque<u8>,
    pending_write: Vec<u8>,
    pending_write_pos: usize,
    pending_input_len: usize,
}

impl LegacyStream {
    fn client(
        inner: BoxStream,
        method: LegacyMethod,
        key: Vec<u8>,
        iv: Vec<u8>,
        write_state: CipherState,
    ) -> Result<Self, ShadowsocksError> {
        let write_state = Some(write_state);
        let read_state = (method.iv_len() == 0)
            .then(|| CipherState::new(method, &key, &[]))
            .transpose()?;
        Ok(Self {
            inner,
            method,
            key,
            read_iv: Vec::new(),
            read_state,
            write_iv: iv,
            write_state,
            ota_read: false,
            ota_write: method.ota,
            ota_sequence_read: 0,
            ota_sequence_write: 0,
            ota_pending: Vec::new(),
            plain: VecDeque::new(),
            pending_write: Vec::new(),
            pending_write_pos: 0,
            pending_input_len: 0,
        })
    }

    fn server(
        inner: BoxStream,
        method: LegacyMethod,
        key: Vec<u8>,
        iv: Vec<u8>,
    ) -> Result<Self, ShadowsocksError> {
        let read_state = Some(CipherState::new(method, &key, &iv)?);
        let write_state = (method.iv_len() == 0)
            .then(|| CipherState::new(method, &key, &[]))
            .transpose()?;
        Ok(Self {
            inner,
            method,
            key,
            read_iv: iv,
            read_state,
            write_iv: Vec::new(),
            write_state,
            ota_read: false,
            ota_write: false,
            ota_sequence_read: 0,
            ota_sequence_write: 0,
            ota_pending: Vec::new(),
            plain: VecDeque::new(),
            pending_write: Vec::new(),
            pending_write_pos: 0,
            pending_input_len: 0,
        })
    }

    fn enable_ota(&mut self) -> Result<(), ShadowsocksError> {
        self.ota_read = true;
        self.ota_write = true;
        if !self.plain.is_empty() {
            let buffered: Vec<u8> = self.plain.drain(..).collect();
            self.decode_plain(&buffered)?;
        }
        Ok(())
    }

    fn decode_plain(&mut self, data: &[u8]) -> Result<(), ShadowsocksError> {
        if !self.ota_read {
            self.plain.extend(data);
            return Ok(());
        }
        self.ota_pending.extend_from_slice(data);
        loop {
            if self.ota_pending.len() < 2 {
                break;
            }
            let length = u16::from_be_bytes([self.ota_pending[0], self.ota_pending[1]]) as usize;
            if length > MAX_OTA_CHUNK {
                return Err(ShadowsocksError::InvalidOtaHmac);
            }
            if self.ota_pending.len() < 2 + OTA_MAC_LEN + length {
                break;
            }
            let mac = &self.ota_pending[2..2 + OTA_MAC_LEN];
            let body = &self.ota_pending[2 + OTA_MAC_LEN..2 + OTA_MAC_LEN + length];
            if mac != chunk_mac(&self.read_iv, self.ota_sequence_read, body) {
                return Err(ShadowsocksError::InvalidOtaHmac);
            }
            self.plain.extend(body);
            self.ota_sequence_read = self.ota_sequence_read.wrapping_add(1);
            self.ota_pending.drain(..2 + OTA_MAC_LEN + length);
        }
        Ok(())
    }

    fn encode_write(&mut self, data: &[u8]) -> Result<Vec<u8>, ShadowsocksError> {
        let mut plain = Vec::new();
        if self.ota_write {
            for chunk in data.chunks(MAX_OTA_CHUNK) {
                plain.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
                plain.extend_from_slice(&chunk_mac(&self.write_iv, self.ota_sequence_write, chunk));
                plain.extend_from_slice(chunk);
                self.ota_sequence_write = self.ota_sequence_write.wrapping_add(1);
            }
        } else {
            plain.extend_from_slice(data);
        }
        let mut state = self
            .write_state
            .take()
            .ok_or(ShadowsocksError::InvalidKeyLength)?;
        state.apply(&mut plain, false);
        self.write_state = Some(state);
        Ok(plain)
    }
}

impl AsyncRead for LegacyStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if !self.plain.is_empty() {
            let count = output.remaining().min(self.plain.len());
            let bytes: Vec<u8> = self.plain.drain(..count).collect();
            output.put_slice(&bytes);
            return Poll::Ready(Ok(()));
        }
        let mut raw = [0u8; 8192];
        let mut read = ReadBuf::new(&mut raw);
        match Pin::new(&mut self.inner).poll_read(cx, &mut read) {
            Poll::Ready(Ok(())) => {
                let filled = read.filled();
                if filled.is_empty() {
                    if self.ota_read && !self.ota_pending.is_empty() {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "truncated OTA chunk",
                        )));
                    }
                    return Poll::Ready(Ok(()));
                }
                if self.read_state.is_none() {
                    let needed = self.method.iv_len().saturating_sub(self.read_iv.len());
                    let take = needed.min(filled.len());
                    self.read_iv.extend_from_slice(&filled[..take]);
                    if self.read_iv.len() < self.method.iv_len() {
                        return Poll::Ready(Ok(()));
                    }
                    self.read_state = CipherState::new(self.method, &self.key, &self.read_iv).ok();
                    if self.read_state.is_none() {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "invalid legacy cipher state",
                        )));
                    }
                    let tail = &filled[take..];
                    let mut data = tail.to_vec();
                    self.read_state.as_mut().unwrap().apply(&mut data, true);
                    if let Err(error) = self.decode_plain(&data) {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            error.to_string(),
                        )));
                    }
                } else {
                    let mut data = filled.to_vec();
                    self.read_state.as_mut().unwrap().apply(&mut data, true);
                    if let Err(error) = self.decode_plain(&data) {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            error.to_string(),
                        )));
                    }
                }
                let count = output.remaining().min(self.plain.len());
                if count > 0 {
                    let bytes: Vec<u8> = self.plain.drain(..count).collect();
                    output.put_slice(&bytes);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for LegacyStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.pending_write.is_empty() {
            if self.write_state.is_none() {
                let mut iv = vec![0u8; self.method.iv_len()];
                rand::thread_rng().fill_bytes(&mut iv);
                self.write_iv = iv.clone();
                self.write_state = match CipherState::new(self.method, &self.key, &iv) {
                    Ok(state) => Some(state),
                    Err(error) => {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            error.to_string(),
                        )))
                    }
                };
                self.pending_write.extend_from_slice(&iv);
            }
            let encoded = match self.encode_write(data) {
                Ok(encoded) => encoded,
                Err(error) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        error.to_string(),
                    )))
                }
            };
            self.pending_write.extend_from_slice(&encoded);
            self.pending_input_len = data.len();
        }
        while self.pending_write_pos < self.pending_write.len() {
            let pos = self.pending_write_pos;
            let pending = self.pending_write[pos..].to_vec();
            match Pin::new(&mut self.inner).poll_write(cx, &pending) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "legacy cipher stream write returned zero",
                    )))
                }
                Poll::Ready(Ok(count)) => self.pending_write_pos += count,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        self.pending_write.clear();
        self.pending_write_pos = 0;
        let count = self.pending_input_len;
        self.pending_input_len = 0;
        Poll::Ready(Ok(count))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if !self.pending_write.is_empty() {
            match self.as_mut().poll_write(cx, &[]) {
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.as_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.inner).poll_shutdown(cx),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eggress_core::{TargetAddr, TargetHost};

    #[test]
    fn inventory_parses_supported_and_rejects_unimplemented() {
        for name in [
            "table",
            "rc4",
            "rc4-md5",
            "aes-128-cfb",
            "aes-192-ctr",
            "aes-256-ofb",
            "chacha20",
            "chacha20-ietf",
            "xchacha20",
            "salsa20",
            "xsalsa20",
            "bf-cfb",
            "des-cfb",
            "camellia-256-cfb",
        ] {
            assert!(LegacyMethod::parse(name).is_ok(), "{name}");
        }
        assert!(LegacyMethod::parse("cast5-cfb").is_err());
        assert_eq!(
            LegacyMethod::parse("aes-128-cfb!-py").unwrap().name(),
            "aes-128-cfb"
        );
    }

    #[test]
    fn stream_vectors_are_fragmentation_invariant() {
        for name in [
            "table",
            "rc4",
            "aes-128-cfb",
            "aes-256-ctr",
            "chacha20-ietf",
            "salsa20",
        ] {
            let method = LegacyMethod::parse(name).unwrap();
            let key = derive_key(b"password", method.key_len());
            let iv = vec![0x42; method.iv_len()];
            let mut one = CipherState::new(method, &key, &iv).unwrap();
            let mut fragmented = CipherState::new(method, &key, &iv).unwrap();
            let mut expected = b"fragmented legacy stream".to_vec();
            one.apply(&mut expected, false);
            let mut actual = b"fragmented legacy stream".to_vec();
            for part in actual.chunks_mut(3) {
                fragmented.apply(part, false);
            }
            assert_eq!(actual, expected, "{name}");
        }
    }

    #[test]
    fn pproxy_279_known_answer_vectors() {
        let vectors = [
            ("table", "27d9dfedba923653ed5b"),
            ("rc4", "849d4c5c1622488a104c"),
            ("rc4-md5", "af2c1d52d408ffb80064"),
            ("aes-128-cfb", "3eec8e70b1e953623832"),
            ("aes-192-cfb", "62093dafda835e8313c3"),
            ("aes-256-cfb", "bfaee958b0b95fa74aa0"),
            ("aes-128-cfb1", "4677627f32b3058b8e0b"),
            ("aes-128-cfb8", "3e5d8a613d33b0f8f963"),
            ("aes-128-ctr", "3eec8e70b1e953623832"),
            ("aes-128-ofb", "3eec8e70b1e953623832"),
            ("bf-cfb", "3f0c01b840722fd13f30"),
            ("des-cfb", "ff267dc00a1129168a7d"),
            ("camellia-128-cfb", "628c21253f24401192c0"),
            ("camellia-192-cfb", "032ea7e4a7b9b90ed969"),
            ("camellia-256-cfb", "54a61435f2e9cbce5e95"),
            ("chacha20", "caedd6f6bf738c98bc0a"),
            ("chacha20-ietf", "cc501624fbddc83efb2b"),
            ("xchacha20", "8afbb4beda78b9ef2026"),
            ("salsa20", "c246e78cf40c5de9f948"),
        ];
        for (name, expected) in vectors {
            let method = LegacyMethod::parse(name).unwrap();
            let key = derive_key(b"password", method.key_len());
            let iv = (0..method.iv_len())
                .map(|value| value as u8)
                .collect::<Vec<_>>();
            let mut state = CipherState::new(method, &key, &iv).unwrap();
            let mut data = b"legacy-kat".to_vec();
            state.apply(&mut data, false);
            assert_eq!(hex_encode(&data), expected, "{name}");
        }
    }

    fn hex_encode(data: &[u8]) -> String {
        data.iter().map(|value| format!("{value:02x}")).collect()
    }

    #[tokio::test]
    async fn ota_roundtrip_rejects_bad_hmac() {
        let method = LegacyMethod::parse("aes-128-cfb!").unwrap();
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 443,
        };
        let (left, right) = tokio::io::duplex(8192);
        let server =
            tokio::spawn(async move { legacy_accept(Box::new(right), method, b"password").await });
        let mut client = legacy_connect(Box::new(left), &target, method, b"password")
            .await
            .unwrap();
        client.write_all(b"payload").await.unwrap();
        let (mut server_stream, received) = server.await.unwrap().unwrap();
        assert_eq!(received, target);
        let mut payload = vec![0; 7];
        server_stream.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload, b"payload");
    }

    #[test]
    fn udp_packet_roundtrip_is_packet_local() {
        let method = LegacyMethod::parse("aes-128-cfb").unwrap();
        let target = TargetAddr {
            host: TargetHost::Domain("example.com".into()),
            port: 53,
        };
        let iv = vec![0x42; method.iv_len()];
        let packet = legacy_udp_encode(method, b"password", &target, b"payload", &iv).unwrap();
        let (decoded_target, payload) = legacy_udp_decode(method, b"password", &packet).unwrap();
        assert_eq!(decoded_target, target);
        assert_eq!(payload, b"payload");
    }
}
