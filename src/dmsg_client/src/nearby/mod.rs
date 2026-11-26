//! `dmsg-nearby-share/1`: face-to-face, end-to-end encrypted cross-device
//! sharing of Files and Vault snapshots (`dmsg-secret-package/1`) over LAN TCP,
//! Apple AWDL byte bridges, and ephemeral blind relays.

pub mod hosted;
pub mod offer;
pub mod relay;
pub mod session;
pub mod wire;

#[cfg(test)]
mod tests;

use std::time::Duration;

/// Protocol format version.
pub const FORMAT: u8 = 1;
/// mDNS / Bonjour service type advertised when discoverability is explicitly enabled.
pub const SERVICE: &str = "_dmsg-share._tcp";
/// URI prefix for QR and copy-paste offers.
pub const OFFER_PREFIX: &str = "dmsg-nearby/1:";
/// Maximum encoded CBOR byte length of a [`offer::NearbyOffer`].
pub const MAX_OFFER_BYTES: usize = 512;
/// Maximum plaintext encrypted frame body (`1 MiB + 64 KiB`).
pub const MAX_BODY: usize = 1024 * 1024 + 64 * 1024;
/// Maximum plaintext handshake message length.
pub const MAX_HANDSHAKE_FRAME: usize = 4096;
/// Maximum total plaintext bytes in one local nearby session (10 GiB).
pub const MAX_SESSION_BYTES: u64 = 10 * 1024 * 1024 * 1024;
/// Maximum files in one local nearby session.
pub const MAX_SESSION_FILES: usize = 64;
/// Maximum payload bytes through the ephemeral blind relay (100 MiB).
pub const RELAY_MAX_BYTES: u64 = 100 * 1024 * 1024;
/// Maximum files through the ephemeral blind relay.
pub const RELAY_MAX_FILES: usize = 20;
/// Server-enforced join TTL for ephemeral blind relay rooms (5 minutes).
pub const RELAY_JOIN_TTL_MS: u64 = 300_000;
/// Advertised offer TTL (270 seconds, leaving 30 seconds for clock skew).
pub const OFFER_TTL_MS: u64 = 270_000;
/// Initial chunk credit window (`W = 4` chunks, at most 4 MiB in flight).
pub const DEFAULT_CREDITS: u32 = 4;
/// Receiver replenishes credits every `N = 2` chunks consumed.
pub const CREDIT_REFILL: u32 = 2;
/// Maximum wait for each plaintext handshake message.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum wait for user confirmation or high-risk approval.
pub const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);
/// Maximum inactivity between encrypted transfer frames.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

pub use hosted::{
    AnyTransport, BridgeHandle, BridgeTransport, DiscoveredPeer, NearbyHost, PendingPeerView,
    SendPayload, SendTarget, listen, local_addresses,
};
pub use offer::{NearbyOffer, RelayOffer, advert, pk_hint, sanitize_alias, validate_local_addr};
pub use relay::{RelayRoom, WsFrameTransport, cancel_room, create_room, ws_url};
pub use session::{
    PeerIdentity, Progress, ReceiveDecision, ReceiveOutcome, ReceivedFile, SendFileItem,
    SendOutcome,
};
pub use wire::{
    Body, Channel, EphemeralKey, FileSummary, FrameTransport, HandshakeSecrets, OfferKind,
};
