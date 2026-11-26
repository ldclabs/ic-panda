//! `dmsg-nearby/1:` QR and copy-paste offers, strict address validation,
//! mDNS advertisement labels and alias sanitization.

use super::{FORMAT, MAX_OFFER_BYTES, OFFER_PREFIX};
use crate::codec::{canonical, decode, digest};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, Bytes};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

const ADVERT_DOMAIN: &str = "dmsg/nearby-share/advert/v1";
const PK_HINT_DOMAIN: &str = "dmsg/nearby-share/pk-hint/v1";

/// Maximum number of candidate local socket addresses in an offer.
pub const MAX_ADDRS: usize = 4;
/// Maximum display alias length in UTF-8 bytes.
pub const MAX_ALIAS_BYTES: usize = 32;

/// Ephemeral relay attachment inside an offer.
///
/// The sender never connects to an arbitrary URL from the QR code: it checks
/// `env` against its own configured environment and connects only to its own
/// configured relay origin.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RelayOffer {
    /// Expected deployment environment (`local`, `staging`, `production`).
    pub env: String,
    /// Ephemeral 16-byte relay room ID.
    pub rid: ByteArray<16>,
    /// One-time 16-byte sender WebSocket bearer token (never used for content encryption).
    pub tok: ByteArray<16>,
}

/// Compact out-of-band offer rendered as a QR code or copied as `dmsg-nearby/1:...` text.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NearbyOffer {
    /// Protocol version (`1`).
    pub v: u8,
    /// Receiver's ephemeral X25519 public key (`R_pk`).
    pub pk: ByteArray<32>,
    /// Random 16-byte QR secret (`qrs`) proving the sender actually scanned or pasted this offer.
    pub qrs: ByteArray<16>,
    /// Advisory join deadline in Unix milliseconds (enforced by receiver and relay, not sender clock).
    pub exp: u64,
    /// Optional 16-hex mDNS instance label advertised under `_dmsg-share._tcp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adv: Option<String>,
    /// Up to 4 candidate local private or link-local `ip:port` endpoints.
    pub addrs: Vec<String>,
    /// Optional ephemeral blind relay fallback coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<RelayOffer>,
}

impl NearbyOffer {
    /// Encode as `dmsg-nearby/1:<base64url(canonical-cbor)>`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` if any field violates offer bounds.
    pub fn encode(&self) -> Result<String> {
        self.encode_for(None)
    }

    /// Encode with environment context (`"local"` permits loopback addresses).
    ///
    /// # Errors
    /// Returns `Error::InvalidInput`, `Error::UnsupportedProtocol`, `Error::Forbidden` or `Error::QuotaExceeded`.
    pub fn encode_for(&self, expected_env: Option<&str>) -> Result<String> {
        self.validate(expected_env)?;
        let bytes = canonical(self);
        ensure(bytes.len() <= MAX_OFFER_BYTES, Error::QuotaExceeded)?;
        Ok(format!("{OFFER_PREFIX}{}", crate::codec::b64(&bytes)))
    }

    /// Decode and validate a `dmsg-nearby/1:...` text offer.
    ///
    /// When `expected_env` is given, any embedded `relay.env` must match it,
    /// and loopback addresses in `addrs` are accepted only in `"local"`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput`, `Error::UnsupportedProtocol` or `Error::Forbidden`.
    pub fn parse(text: &str, expected_env: Option<&str>) -> Result<Self> {
        let trimmed = text.trim();
        let encoded = trimmed
            .strip_prefix(OFFER_PREFIX)
            .ok_or_else(|| Error::InvalidInput("nearby offer prefix".into()))?;
        let raw = crate::codec::unb64(encoded)?;
        ensure(raw.len() <= MAX_OFFER_BYTES, Error::QuotaExceeded)?;
        let offer: Self = decode(&raw, MAX_OFFER_BYTES)?;
        offer.validate(expected_env)?;
        Ok(offer)
    }

    /// Validate wire bounds, address safety and environment alignment.
    ///
    /// # Errors
    /// Rejects non-private/public IPs, hostnames, mismatched relay environment or oversize fields.
    pub fn validate(&self, expected_env: Option<&str>) -> Result<()> {
        ensure(self.v == FORMAT, Error::UnsupportedProtocol)?;
        ensure(self.exp > 0, Error::InvalidInput("nearby offer exp".into()))?;
        ensure(
            self.addrs.len() <= MAX_ADDRS,
            Error::InvalidInput("too many nearby addresses".into()),
        )?;
        let allow_loopback = matches!(expected_env, Some("local"))
            || matches!(self.relay.as_ref().map(|r| r.env.as_str()), Some("local"));
        for addr in &self.addrs {
            validate_local_addr(addr, allow_loopback)?;
        }
        if let Some(adv) = &self.adv {
            ensure(
                adv.len() == 16 && adv.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
                Error::InvalidInput("nearby advert".into()),
            )?;
        }
        if let Some(relay) = &self.relay {
            ensure(
                matches!(relay.env.as_str(), "local" | "staging" | "production"),
                Error::InvalidInput("nearby relay env".into()),
            )?;
            if let Some(expected) = expected_env {
                ensure(relay.env == expected, Error::Forbidden)?;
            }
        }
        ensure(
            !self.addrs.is_empty() || self.adv.is_some() || self.relay.is_some(),
            Error::InvalidInput("nearby offer has no reachable endpoint".into()),
        )?;
        Ok(())
    }

    /// Parse all validated `addrs` into `SocketAddr` values.
    pub fn socket_addrs(&self) -> Vec<SocketAddr> {
        self.addrs
            .iter()
            .filter_map(|text| text.parse::<SocketAddr>().ok())
            .collect()
    }
}

/// Validate that `addr` is a literal private or link-local `ip:port` socket address.
///
/// Public IPs, hostnames, multicast, unspecified addresses and port 0 are rejected
/// so a QR code can never steer a client to probe external hosts. Loopback is
/// permitted only in `"local"` deployments for local testing.
pub fn validate_local_addr(addr: &str, allow_loopback: bool) -> Result<SocketAddr> {
    ensure(
        addr.len() <= 64
            && !addr
                .bytes()
                .any(|b| b.is_ascii_control() || b.is_ascii_whitespace()),
        Error::InvalidInput("nearby address".into()),
    )?;
    let parsed: SocketAddr = addr
        .parse()
        .map_err(|_| Error::InvalidInput("nearby address must be literal ip:port".into()))?;
    ensure(parsed.port() > 0, Error::InvalidInput("nearby port".into()))?;
    ensure(
        is_allowed_local_ip(parsed.ip(), allow_loopback),
        Error::Forbidden,
    )?;
    Ok(parsed)
}

/// Whether `ip` is RFC 1918 private, RFC 4193 ULA, or link-local (or loopback in `local` env).
pub fn is_allowed_local_ip(ip: IpAddr, allow_loopback: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => is_allowed_v4(v4, allow_loopback),
        IpAddr::V6(v6) => is_allowed_v6(v6, allow_loopback),
    }
}

fn is_allowed_v4(ip: Ipv4Addr, allow_loopback: bool) -> bool {
    if ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() {
        return false;
    }
    if ip.is_loopback() {
        return allow_loopback;
    }
    ip.is_private() || ip.is_link_local()
}

fn is_allowed_v6(ip: Ipv6Addr, allow_loopback: bool) -> bool {
    if ip.is_unspecified() || ip.is_multicast() {
        return false;
    }
    if ip.is_loopback() {
        return allow_loopback;
    }
    let seg0 = ip.segments()[0];
    // RFC 4193 Unique Local (fc00::/7) or link-local (fe80::/10).
    (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
}

/// Ephemeral mDNS instance name for session `sid`: 16 lowercase hex chars.
pub fn advert(sid: &[u8; 16]) -> String {
    hex::encode(&digest(ADVERT_DOMAIN, &Bytes::new(sid))[..8])
}

/// Short base64url public-key hint (`p=...` TXT attribute, 4 bytes) to tell
/// same-named nearby receivers apart without revealing account identity.
pub fn pk_hint(r_pk: &[u8; 32]) -> String {
    let tag = digest(PK_HINT_DOMAIN, &Bytes::new(r_pk));
    crate::codec::b64(&tag[..4])
}

/// Sanitize a user-chosen or default nearby display alias to at most 32 UTF-8
/// bytes with control characters stripped.
pub fn sanitize_alias(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.trim().chars() {
        if ch.is_control() {
            continue;
        }
        let next_len = out.len() + ch.len_utf8();
        if next_len > MAX_ALIAS_BYTES {
            break;
        }
        out.push(ch);
    }
    if out.is_empty() {
        "dMsg Device".into()
    } else {
        out
    }
}

/// Encode `RelayOffer::tok` as unpadded base64url for `Authorization: Bearer <token>`.
pub fn bearer_token(token: &[u8; 16]) -> String {
    crate::codec::b64(token)
}
