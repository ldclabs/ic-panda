//! Derivation paths and address encodings: ICRC accounts, Bitcoin bech32,
//! bech32m and base58check, Taproot trees, EVM EIP-55 and Solana base58.
use crate::sha256;
use candid::Principal;
use dmsg_types::{wallet::*, *};
use icrc_ledger_types::icrc1::account::Account;
use k256::elliptic_curve::{
    ff::PrimeField, group::Group, point::AffineCoordinates, sec1::ToSec1Point,
};
use sha2::{Digest, Sha256};
use sha3::Keccak256;

/// First segment of every derivation path.
pub const DERIVATION_PREFIX: &[u8] = b"dmsg-wallet/v1";
/// BIP-341 "nothing up my sleeve" point: the internal key of native Taproot
/// addresses, whose key path no one can spend.
pub const NUMS_KEY: [u8; 32] = [
    0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a, 0x5e,
    0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80, 0x3a, 0xc0,
];
/// Tapscript leaf version.
pub const TAPSCRIPT_LEAF: u8 = 0xc0;

/// Threshold key an address signs with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAlgorithm {
    /// Threshold ECDSA secp256k1.
    Ecdsa,
    /// Threshold Schnorr BIP-340.
    Bip340,
    /// Threshold Schnorr Ed25519.
    Ed25519,
}

/// The key of an address, or `None` for ICP, which needs no signature.
///
/// # Errors
/// Returns `Error::InvalidInput` for a kind the chain does not support.
pub fn key_algorithm(chain: Chain, kind: &AddressKind) -> Result<Option<KeyAlgorithm>> {
    Ok(match (chain, kind) {
        (Chain::Icp, AddressKind::ChainKey) => None,
        (Chain::Bitcoin, AddressKind::ChainKey | AddressKind::Taproot(_)) => {
            Some(KeyAlgorithm::Bip340)
        }
        (Chain::Bitcoin, AddressKind::Segwit) | (Chain::Evm, AddressKind::ChainKey) => {
            Some(KeyAlgorithm::Ecdsa)
        }
        (Chain::Solana, AddressKind::ChainKey) => Some(KeyAlgorithm::Ed25519),
        _ => return Err(invalid("address kind for chain")),
    })
}

/// `[b"dmsg-wallet/v1", chain tag, id big-endian]`; every EVM chain shares
/// the `eth` key.
///
/// # Errors
/// As [`key_algorithm`].
pub fn derivation_path(chain: Chain, kind: &AddressKind, id: u32) -> Result<Vec<Vec<u8>>> {
    let tag: &[u8] = match (chain, kind) {
        (Chain::Bitcoin, AddressKind::Segwit) => b"btc-segwit",
        (Chain::Bitcoin, AddressKind::ChainKey) => b"btc",
        (Chain::Bitcoin, AddressKind::Taproot(_)) => b"btc-native",
        (Chain::Evm, AddressKind::ChainKey) => b"eth",
        (Chain::Solana, AddressKind::ChainKey) => b"sol",
        (Chain::Icp, AddressKind::ChainKey) => b"icp",
        _ => return Err(invalid("address kind for chain")),
    };
    Ok(vec![
        DERIVATION_PREFIX.to_vec(),
        tag.to_vec(),
        id.to_be_bytes().to_vec(),
    ])
}

/// The ICRC subaccount of ICP address `id`: zero for address 0.
pub fn icp_subaccount(id: u32) -> Option<[u8; 32]> {
    (id != 0).then(|| {
        let mut sub = [0; 32];
        sub[28..].copy_from_slice(&id.to_be_bytes());
        sub
    })
}

/// The ICRC account of ICP address `id`.
pub fn icp_account(wallet: Principal, id: u32) -> Account {
    Account {
        owner: wallet,
        subaccount: icp_subaccount(id),
    }
}

/// `SHA256(SHA256(tag) || SHA256(tag) || data)`.
pub fn tagged_hash(tag: &str, data: &[u8]) -> [u8; 32] {
    let tag = Sha256::digest(tag.as_bytes());
    Sha256::new()
        .chain_update(tag)
        .chain_update(tag)
        .chain_update(data)
        .finalize()
        .into()
}

fn lift_x(x: &[u8; 32]) -> Result<k256::AffinePoint> {
    let key =
        k256::schnorr::VerifyingKey::from_bytes(x.into()).map_err(|_| Error::IntegrityFailed)?;
    Ok(*key.as_affine())
}

/// x-only key of a SEC1 compressed secp256k1 key.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a malformed key.
pub fn x_only(sec1: &[u8]) -> Result<[u8; 32]> {
    let key = k256::PublicKey::from_sec1_bytes(sec1).map_err(|_| Error::IntegrityFailed)?;
    Ok(key.as_affine().x().into())
}

/// BIP-341 output key of internal key `internal` committing to `merkle_root`
/// (none for a key-path-only output), with the parity of its y coordinate.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for an invalid key or tweak.
pub fn taproot_output_key(
    internal: &[u8; 32],
    merkle_root: Option<&[u8; 32]>,
) -> Result<([u8; 32], bool)> {
    let p = lift_x(internal)?;
    let mut data = internal.to_vec();
    if let Some(root) = merkle_root {
        data.extend_from_slice(root);
    }
    let t: Option<k256::Scalar> =
        k256::Scalar::from_repr(tagged_hash("TapTweak", &data).into()).into();
    let t = t.ok_or(Error::IntegrityFailed)?;
    let q = (k256::ProjectivePoint::from(p) + k256::ProjectivePoint::generator() * t).to_affine();
    ensure(
        !bool::from(k256::ProjectivePoint::from(q).is_identity()),
        Error::IntegrityFailed,
    )?;
    Ok((q.x().into(), bool::from(q.y_is_odd())))
}

/// Minimal script push of a small positive number.
pub fn script_number(n: u16) -> Vec<u8> {
    match n {
        0 => vec![0x00],
        1..=16 => vec![0x50 + n as u8],
        _ => {
            let mut bytes = n.to_le_bytes().to_vec();
            while bytes.last() == Some(&0) {
                bytes.pop();
            }
            if bytes.last().is_some_and(|b| b & 0x80 != 0) {
                bytes.push(0);
            }
            [vec![bytes.len() as u8], bytes].concat()
        }
    }
}

/// Bitcoin compact size.
pub fn compact_size(n: usize) -> Vec<u8> {
    match n {
        0..=0xfc => vec![n as u8],
        0xfd..=0xffff => [vec![0xfd], (n as u16).to_le_bytes().to_vec()].concat(),
        _ => [vec![0xfe], (n as u32).to_le_bytes().to_vec()].concat(),
    }
}

/// Leaf scripts of a native Taproot address, in tree order.
pub struct TaprootTree {
    /// Leaf scripts: two-key, chain-key after delay, optional recovery.
    pub leaves: Vec<Vec<u8>>,
    /// Merkle root.
    pub root: [u8; 32],
}

/// Leaf hash of a tapscript.
pub fn tapleaf_hash(script: &[u8]) -> [u8; 32] {
    let mut data = vec![TAPSCRIPT_LEAF];
    data.extend(compact_size(script.len()));
    data.extend_from_slice(script);
    tagged_hash("TapLeaf", &data)
}

fn tap_branch(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (l, r) = if a <= b { (a, b) } else { (b, a) };
    tagged_hash("TapBranch", &[&l[..], &r[..]].concat())
}

/// Script tree of a native Taproot address: `{L1, L2}` or `{L1, {L2, L3}}`.
pub fn taproot_tree(
    chain_key: &[u8; 32],
    cosigner: &[u8; 32],
    control: &TaprootControl,
) -> TaprootTree {
    let checksig = |key: &[u8; 32]| [&[0x20][..], key, &[0xac]].concat();
    let after =
        |n: u16, key: &[u8; 32]| [script_number(n), vec![0xb2, 0x75], checksig(key)].concat();
    let mut leaves = vec![
        [&[0x20][..], chain_key, &[0xad, 0x20], cosigner, &[0xac]].concat(),
        after(control.chain_key_after, chain_key),
    ];
    if let Some(r) = &control.recovery {
        leaves.push(after(r.after, &r.key.into_array()));
    }
    let hashes: Vec<[u8; 32]> = leaves.iter().map(|l| tapleaf_hash(l)).collect();
    let root = match hashes.as_slice() {
        [a, b] => tap_branch(a, b),
        [a, b, c] => tap_branch(a, &tap_branch(b, c)),
        _ => unreachable!("two or three leaves"),
    };
    TaprootTree { leaves, root }
}

/// Control block of leaf `index` of a native Taproot tree.
pub fn taproot_control_block(tree: &TaprootTree, index: usize, output_odd: bool) -> Vec<u8> {
    let hashes: Vec<[u8; 32]> = tree.leaves.iter().map(|l| tapleaf_hash(l)).collect();
    let path: Vec<[u8; 32]> = match (hashes.as_slice(), index) {
        ([_, b], 0) => vec![*b],
        ([a, _], 1) => vec![*a],
        ([_, b, c], 0) => vec![tap_branch(b, c)],
        ([a, _, c], 1) => vec![*c, *a],
        ([a, b, _], 2) => vec![*b, *a],
        _ => vec![],
    };
    let mut block = vec![TAPSCRIPT_LEAF | u8::from(output_odd)];
    block.extend_from_slice(&NUMS_KEY);
    for h in path {
        block.extend_from_slice(&h);
    }
    block
}

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BECH32M: u32 = 0x2bc8_30a3;

fn polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    let mut chk: u32 = 1;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01ff_ffff) << 5) ^ u32::from(*v);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut v: Vec<u8> = hrp.bytes().map(|c| c >> 5).collect();
    v.push(0);
    v.extend(hrp.bytes().map(|c| c & 31));
    v
}

fn convert_bits(data: &[u8], from: u32, to: u32, pad: bool) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let max = (1u32 << to) - 1;
    let mut out = vec![];
    for v in data {
        let v = u32::from(*v);
        if v >> from != 0 {
            return None;
        }
        acc = (acc << from) | v;
        bits += from;
        while bits >= to {
            bits -= to;
            out.push(((acc >> bits) & max) as u8);
        }
    }
    if pad {
        if bits > 0 {
            out.push(((acc << (to - bits)) & max) as u8);
        }
    } else if bits >= from || ((acc << (to - bits)) & max) != 0 {
        return None;
    }
    Some(out)
}

/// Human-readable part of segwit addresses on `network`.
pub fn btc_hrp(network: BtcNetwork) -> &'static str {
    match network {
        BtcNetwork::Mainnet => "bc",
        BtcNetwork::Testnet => "tb",
        BtcNetwork::Regtest => "bcrt",
    }
}

/// Segwit address: bech32 for version 0, bech32m for later versions.
pub fn segwit_address(network: BtcNetwork, version: u8, program: &[u8]) -> String {
    let hrp = btc_hrp(network);
    let mut data = vec![version];
    data.extend(convert_bits(program, 8, 5, true).expect("bytes"));
    let constant = if version == 0 { 1 } else { BECH32M };
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(&data);
    values.extend_from_slice(&[0; 6]);
    let pm = polymod(&values) ^ constant;
    data.extend((0..6).map(|i| ((pm >> (5 * (5 - i))) & 31) as u8));
    let mut s = format!("{hrp}1");
    s.extend(data.iter().map(|d| CHARSET[*d as usize] as char));
    s
}

fn decode_segwit(network: BtcNetwork, text: &str) -> Option<(u8, Vec<u8>)> {
    if text.len() > 90 || (text.to_lowercase() != text && text.to_uppercase() != text) {
        return None;
    }
    let text = text.to_lowercase();
    let split = text.rfind('1')?;
    let (hrp, rest) = (&text[..split], &text[split + 1..]);
    if hrp != btc_hrp(network) || rest.len() < 7 {
        return None;
    }
    let data: Vec<u8> = rest
        .bytes()
        .map(|c| CHARSET.iter().position(|x| *x == c).map(|p| p as u8))
        .collect::<Option<_>>()?;
    let mut values = hrp_expand(hrp);
    values.extend_from_slice(&data);
    let check = polymod(&values);
    let version = data[0];
    if version > 16 || check != if version == 0 { 1 } else { BECH32M } {
        return None;
    }
    let program = convert_bits(&data[1..data.len() - 6], 5, 8, false)?;
    let valid = match version {
        0 => program.len() == 20 || program.len() == 32,
        _ => (2..=40).contains(&program.len()),
    };
    valid.then_some((version, program))
}

fn double_sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(bytes)).into()
}

fn decode_base58check(network: BtcNetwork, text: &str) -> Option<Vec<u8>> {
    let bytes = bs58::decode(text).into_vec().ok()?;
    if bytes.len() != 25 || double_sha256(&bytes[..21])[..4] != bytes[21..] {
        return None;
    }
    let hash = &bytes[1..21];
    let (p2pkh, p2sh) = match network {
        BtcNetwork::Mainnet => (0x00, 0x05),
        _ => (0x6f, 0xc4),
    };
    Some(if bytes[0] == p2pkh {
        [&[0x76, 0xa9, 0x14][..], hash, &[0x88, 0xac]].concat()
    } else if bytes[0] == p2sh {
        [&[0xa9, 0x14][..], hash, &[0x87]].concat()
    } else {
        return None;
    })
}

/// Output script of a Bitcoin address on `network`: P2PKH, P2SH or any
/// segwit version.
///
/// # Errors
/// Returns `Error::InvalidInput` for an address of another network or
/// an invalid checksum.
pub fn btc_script(network: BtcNetwork, text: &str) -> Result<Vec<u8>> {
    if let Some((version, program)) = decode_segwit(network, text) {
        let op = if version == 0 { 0 } else { 0x50 + version };
        return Ok([vec![op, program.len() as u8], program].concat());
    }
    decode_base58check(network, text).ok_or_else(|| invalid("bitcoin address"))
}

/// Display form of an output script, if it is a standard one.
pub fn btc_script_address(network: BtcNetwork, script: &[u8]) -> Option<String> {
    let (p2pkh, p2sh) = match network {
        BtcNetwork::Mainnet => (0x00u8, 0x05u8),
        _ => (0x6f, 0xc4),
    };
    let base58 = |version: u8, hash: &[u8]| {
        let mut bytes = vec![version];
        bytes.extend_from_slice(hash);
        let check = double_sha256(&bytes);
        bytes.extend_from_slice(&check[..4]);
        bs58::encode(bytes).into_string()
    };
    match script {
        [0x76, 0xa9, 0x14, hash @ .., 0x88, 0xac] if hash.len() == 20 => Some(base58(p2pkh, hash)),
        [0xa9, 0x14, hash @ .., 0x87] if hash.len() == 20 => Some(base58(p2sh, hash)),
        [op, len, program @ ..]
            if (*op == 0 || (0x51..=0x60).contains(op))
                && usize::from(*len) == program.len()
                && (2..=40).contains(&program.len()) =>
        {
            let version = if *op == 0 { 0 } else { op - 0x50 };
            Some(segwit_address(network, version, program))
        }
        _ => None,
    }
}

/// HASH160 of a public key.
pub fn hash160(bytes: &[u8]) -> [u8; 20] {
    use ripemd::Ripemd160;
    Ripemd160::digest(Sha256::digest(bytes)).into()
}

/// EVM address of a SEC1 secp256k1 key.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a malformed key.
pub fn evm_address(sec1: &[u8]) -> Result<[u8; 20]> {
    let key = k256::PublicKey::from_sec1_bytes(sec1).map_err(|_| Error::IntegrityFailed)?;
    let point = key.as_affine().to_sec1_point(false);
    let hash = Keccak256::digest(&point.as_bytes()[1..]);
    Ok(hash[12..].try_into().expect("20 bytes"))
}

/// EIP-55 mixed-case hex of an EVM address.
pub fn eip55(address: &[u8; 20]) -> String {
    let lower = hex_lower(address);
    let hash = Keccak256::digest(lower.as_bytes());
    let mut out = String::from("0x");
    for (i, c) in lower.chars().enumerate() {
        let nibble = (hash[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0x0f;
        out.push(if c.is_ascii_alphabetic() && nibble >= 8 {
            c.to_ascii_uppercase()
        } else {
            c
        });
    }
    out
}

/// Parse a `0x` EVM address; mixed case must be valid EIP-55.
///
/// # Errors
/// Returns `Error::InvalidInput` for malformed hex or a wrong checksum.
pub fn parse_evm_address(text: &str) -> Result<[u8; 20]> {
    let hex = text
        .strip_prefix("0x")
        .ok_or_else(|| invalid("evm address"))?;
    ensure_valid(hex.len() == 40, "evm address")?;
    let mut out = [0u8; 20];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).map_err(|_| invalid("evm address"))?;
        out[i] = u8::from_str_radix(s, 16).map_err(|_| invalid("evm address"))?;
    }
    let mixed =
        hex.chars().any(|c| c.is_ascii_uppercase()) && hex.chars().any(|c| c.is_ascii_lowercase());
    ensure_valid(!mixed || eip55(&out) == text, "evm address checksum")?;
    Ok(out)
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Solana base58 address.
pub fn sol_address(key: &[u8; 32]) -> String {
    bs58::encode(key).into_string()
}

/// Parse a Solana base58 address.
///
/// # Errors
/// Returns `Error::InvalidInput` unless it decodes to 32 bytes.
pub fn parse_sol_address(text: &str) -> Result<[u8; 32]> {
    bs58::decode(text)
        .into_vec()
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("solana address"))
}

/// Display form of an address from its chain-key public key; `cosigner_key`
/// is the x-only key of a native Taproot address's cosigner.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a malformed key and
/// `Error::InvalidInput` for an unsupported kind.
pub fn address_text(
    wallet: Principal,
    network: BtcNetwork,
    id: u32,
    chain: Chain,
    kind: &AddressKind,
    public_key: &[u8],
    cosigner_key: Option<&[u8; 32]>,
) -> Result<String> {
    Ok(match (chain, kind) {
        (Chain::Icp, AddressKind::ChainKey) => icp_account(wallet, id).to_string(),
        (Chain::Bitcoin, AddressKind::ChainKey) => {
            let (q, _) = taproot_output_key(&x_only(public_key)?, None)?;
            segwit_address(network, 1, &q)
        }
        (Chain::Bitcoin, AddressKind::Segwit) => {
            k256::PublicKey::from_sec1_bytes(public_key).map_err(|_| Error::IntegrityFailed)?;
            segwit_address(network, 0, &hash160(public_key))
        }
        (Chain::Bitcoin, AddressKind::Taproot(control)) => {
            let cosigner = cosigner_key.ok_or_else(|| invalid("taproot cosigner"))?;
            let tree = taproot_tree(&x_only(public_key)?, cosigner, control);
            let (q, _) = taproot_output_key(&NUMS_KEY, Some(&tree.root))?;
            segwit_address(network, 1, &q)
        }
        (Chain::Evm, AddressKind::ChainKey) => eip55(&evm_address(public_key)?),
        (Chain::Solana, AddressKind::ChainKey) => {
            let key: [u8; 32] = public_key.try_into().map_err(|_| Error::IntegrityFailed)?;
            sol_address(&key)
        }
        _ => return Err(invalid("address kind for chain")),
    })
}

/// The output script of a wallet Bitcoin address.
///
/// # Errors
/// As [`address_text`].
pub fn btc_address_script(
    network: BtcNetwork,
    address: &Address,
    cosigner_key: Option<&[u8; 32]>,
) -> Result<Vec<u8>> {
    let text = address_text(
        Principal::anonymous(),
        network,
        address.id,
        address.chain,
        &address.kind,
        &address.public_key,
        cosigner_key,
    )?;
    btc_script(network, &text)
}

/// Display form of a destination.
pub fn destination_text(to: &Destination) -> String {
    match to {
        Destination::Icrc(account) => account.to_string(),
        Destination::Bitcoin(text) => text.clone(),
        Destination::Evm(address) => eip55(&address.into_array()),
        Destination::Solana(key) => sol_address(&key.into_array()),
    }
}

/// SHA-256 of a hash, as Bitcoin txids are double SHA-256.
pub fn sha256d(bytes: &[u8]) -> Hash {
    sha256(&Sha256::digest(bytes))
}
