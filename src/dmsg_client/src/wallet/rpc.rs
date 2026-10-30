//! Chain endpoints a client reads and broadcasts through: an Esplora API for
//! Bitcoin, JSON-RPC for EVM chains and Solana. Nothing read here is
//! trusted: the wallet signs only what it builds from the approved intent,
//! and a wrong UTXO value, nonce or blockhash only makes a transaction
//! invalid on chain.
use crate::config::WalletSettings;
use base64::Engine;
use dmsg_types::{Error, Hash, Result, ensure};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

/// Largest answer read from a chain endpoint.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

fn unavailable(e: impl std::fmt::Display) -> Error {
    Error::Unavailable(format!("chain: {e}"))
}

/// The body of an answer, within [`MAX_RESPONSE`].
async fn body(mut r: reqwest::Response) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    while let Some(chunk) = r.chunk().await.map_err(unavailable)? {
        ensure(
            data.len() + chunk.len() <= MAX_RESPONSE,
            Error::QuotaExceeded,
        )?;
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}

/// A JSON answer, within [`MAX_RESPONSE`].
async fn json_body<T: DeserializeOwned>(r: reqwest::Response) -> Result<T> {
    serde_json::from_slice(&body(r).await?).map_err(unavailable)
}

/// An unspent output as Esplora lists it.
#[derive(Deserialize, Clone, Debug)]
pub struct EsploraUtxo {
    /// Displayed transaction ID.
    pub txid: String,
    /// Output index.
    pub vout: u32,
    /// Satoshis.
    pub value: u64,
    /// Where it was confirmed.
    #[serde(default)]
    pub status: EsploraStatus,
}

/// The confirmation of a transaction.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct EsploraStatus {
    /// Whether a block holds it.
    #[serde(default)]
    pub confirmed: bool,
    /// That block's height.
    pub block_height: Option<u32>,
}

/// The chain endpoints of one deployment; clones share one HTTP client.
#[derive(Clone)]
pub struct Chains {
    http: reqwest::Client,
    settings: WalletSettings,
}

impl Chains {
    /// A reader over `settings`' endpoints.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the HTTP client cannot be built.
    pub fn new(settings: &WalletSettings) -> Result<Self> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(unavailable)?;
        Ok(Self {
            http,
            settings: settings.clone(),
        })
    }

    async fn get(&self, url: &str) -> Result<reqwest::Response> {
        let r = self.http.get(url).send().await.map_err(unavailable)?;
        ensure(
            r.status().is_success(),
            Error::Unavailable(format!("chain: {} {url}", r.status())),
        )?;
        Ok(r)
    }

    /// Unspent outputs of a Bitcoin address.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the API fails.
    pub async fn btc_utxos(&self, address: &str) -> Result<Vec<EsploraUtxo>> {
        let url = format!("{}/address/{address}/utxo", self.settings.bitcoin_api);
        json_body(self.get(&url).await?).await
    }

    /// The height of the chain's tip.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the API fails.
    pub async fn btc_tip_height(&self) -> Result<u32> {
        let url = format!("{}/blocks/tip/height", self.settings.bitcoin_api);
        let text = body(self.get(&url).await?).await?;
        std::str::from_utf8(&text)
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .ok_or_else(|| unavailable("tip height"))
    }

    /// A fee rate in satoshis per virtual byte for confirmation within a
    /// few blocks.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the API fails.
    pub async fn btc_fee_rate(&self) -> Result<u64> {
        let url = format!("{}/fee-estimates", self.settings.bitcoin_api);
        let rates: Value = json_body(self.get(&url).await?).await?;
        let rate = ["3", "2", "1", "6"]
            .iter()
            .find_map(|k| rates[*k].as_f64())
            .unwrap_or(2.0);
        Ok((rate.ceil() as u64).clamp(1, 1_000))
    }

    /// Confirmed and unconfirmed balance of a Bitcoin address.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the API fails.
    pub async fn btc_balance(&self, address: &str) -> Result<i128> {
        let url = format!("{}/address/{address}", self.settings.bitcoin_api);
        let v: Value = json_body(self.get(&url).await?).await?;
        let sum = |s: &Value| {
            s["funded_txo_sum"].as_i64().unwrap_or(0) - s["spent_txo_sum"].as_i64().unwrap_or(0)
        };
        Ok(i128::from(
            sum(&v["chain_stats"]) + sum(&v["mempool_stats"]),
        ))
    }

    /// Broadcast a signed Bitcoin transaction; returns its ID.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the network refuses it.
    pub async fn btc_broadcast(&self, raw: &[u8]) -> Result<String> {
        let url = format!("{}/tx", self.settings.bitcoin_api);
        let r = self
            .http
            .post(url)
            .body(hex::encode(raw))
            .send()
            .await
            .map_err(unavailable)?;
        let ok = r.status().is_success();
        let text = String::from_utf8_lossy(&body(r).await?).into_owned();
        ensure(ok, Error::Unavailable(format!("bitcoin: {text}")))?;
        Ok(text.trim().to_string())
    }

    /// Whether a Bitcoin transaction is confirmed.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the API fails.
    pub async fn btc_confirmed(&self, txid: &str) -> Result<bool> {
        let url = format!("{}/tx/{txid}/status", self.settings.bitcoin_api);
        let v: Value = json_body(self.get(&url).await?).await?;
        Ok(v["confirmed"].as_bool().unwrap_or(false))
    }

    async fn rpc(&self, url: &str, method: &str, params: Value) -> Result<Value> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let r = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(unavailable)?;
        let v: Value = json_body(r).await?;
        if let Some(error) = v.get("error").filter(|e| !e.is_null()) {
            return Err(Error::Unavailable(format!("{method}: {error}")));
        }
        Ok(v["result"].clone())
    }

    fn evm_url(&self, chain_id: u64) -> Result<&str> {
        self.settings
            .evm_rpc
            .iter()
            .find(|(c, _)| *c == chain_id)
            .map(|(_, url)| url.as_str())
            .ok_or_else(|| Error::Unavailable(format!("no RPC for EVM chain {chain_id}")))
    }

    /// An EVM JSON-RPC call.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` without an endpoint or when the call fails.
    pub async fn evm(&self, chain_id: u64, method: &str, params: Value) -> Result<Value> {
        let url = self.evm_url(chain_id)?.to_string();
        self.rpc(&url, method, params).await
    }

    /// The next nonce of an address, pending transactions included.
    ///
    /// # Errors
    /// As [`Self::evm`].
    pub async fn evm_nonce(&self, chain_id: u64, address: &str) -> Result<u64> {
        let v = self
            .evm(
                chain_id,
                "eth_getTransactionCount",
                json!([address, "pending"]),
            )
            .await?;
        quantity(&v).and_then(|n| u64::try_from(n).map_err(unavailable))
    }

    /// `(max_fee_per_gas, max_priority_fee_per_gas)`: twice the latest base
    /// fee plus the suggested tip.
    ///
    /// # Errors
    /// As [`Self::evm`].
    pub async fn evm_fees(&self, chain_id: u64) -> Result<(u128, u128)> {
        let (block, tip) = futures_util::try_join!(
            self.evm(chain_id, "eth_getBlockByNumber", json!(["latest", false])),
            self.evm(chain_id, "eth_maxPriorityFeePerGas", json!([]))
        )?;
        let base = quantity(&block["baseFeePerGas"])?;
        let tip = quantity(&tip)?;
        Ok((base * 2 + tip, tip))
    }

    /// Gas for a call, with a fifth more as margin.
    ///
    /// # Errors
    /// As [`Self::evm`].
    pub async fn evm_gas(
        &self,
        chain_id: u64,
        from: &str,
        to: &str,
        data: &[u8],
        value: u128,
    ) -> Result<u64> {
        let call = json!({"from": from, "to": to, "data": format!("0x{}", hex::encode(data)), "value": format!("0x{value:x}")});
        let gas = quantity(&self.evm(chain_id, "eth_estimateGas", json!([call])).await?)?;
        u64::try_from(gas * 6 / 5).map_err(unavailable)
    }

    /// Native or ERC-20 balance.
    ///
    /// # Errors
    /// As [`Self::evm`].
    pub async fn evm_balance(
        &self,
        chain_id: u64,
        address: &str,
        token: Option<&str>,
    ) -> Result<u128> {
        let v = match token {
            None => {
                self.evm(chain_id, "eth_getBalance", json!([address, "latest"]))
                    .await?
            }
            Some(token) => {
                let data = format!(
                    "0x70a08231{:0>64}",
                    address.trim_start_matches("0x").to_lowercase()
                );
                self.evm(
                    chain_id,
                    "eth_call",
                    json!([{"to": token, "data": data}, "latest"]),
                )
                .await?
            }
        };
        quantity(&v)
    }

    /// Broadcast a signed EVM transaction; returns its hash.
    ///
    /// # Errors
    /// As [`Self::evm`].
    pub async fn evm_send(&self, chain_id: u64, raw: &[u8]) -> Result<String> {
        let v = self
            .evm(
                chain_id,
                "eth_sendRawTransaction",
                json!([format!("0x{}", hex::encode(raw))]),
            )
            .await?;
        v.as_str()
            .map(str::to_string)
            .ok_or_else(|| unavailable("hash"))
    }

    /// `Some(succeeded)` once a transaction is mined.
    ///
    /// # Errors
    /// As [`Self::evm`].
    pub async fn evm_receipt(&self, chain_id: u64, hash: &str) -> Result<Option<bool>> {
        let v = self
            .evm(chain_id, "eth_getTransactionReceipt", json!([hash]))
            .await?;
        if v.is_null() {
            return Ok(None);
        }
        Ok(Some(quantity(&v["status"])? == 1))
    }

    /// A Solana JSON-RPC call.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the call fails.
    pub async fn sol(&self, method: &str, params: Value) -> Result<Value> {
        let url = self.settings.solana_rpc.clone();
        self.rpc(&url, method, params).await
    }

    /// The latest blockhash.
    ///
    /// # Errors
    /// As [`Self::sol`].
    pub async fn sol_blockhash(&self) -> Result<Hash> {
        let v = self
            .sol("getLatestBlockhash", json!([{"commitment": "confirmed"}]))
            .await?;
        decode_sol(v["value"]["blockhash"].as_str().unwrap_or_default())
    }

    /// The durable nonce value a nonce account holds.
    ///
    /// # Errors
    /// As [`Self::sol`]; a missing or uninitialized account returns `Error::NotFound`.
    pub async fn sol_nonce(&self, account: &str) -> Result<Hash> {
        let v = self
            .sol(
                "getAccountInfo",
                json!([account, {"encoding": "base64", "commitment": "confirmed"}]),
            )
            .await?;
        let data = v["value"]["data"][0].as_str().ok_or(Error::NotFound)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(unavailable)?;
        // Versions(u32), State(u32 = 1 initialized), authority, durable nonce.
        ensure(
            bytes.len() >= 72 && bytes[4..8] == 1u32.to_le_bytes(),
            Error::NotFound,
        )?;
        Ok(Hash::new(bytes[40..72].try_into().expect("32")))
    }

    /// Whether an account exists.
    ///
    /// # Errors
    /// As [`Self::sol`].
    pub async fn sol_exists(&self, account: &str) -> Result<bool> {
        let v = self
            .sol("getAccountInfo", json!([account, {"encoding": "base64"}]))
            .await?;
        Ok(!v["value"].is_null())
    }

    /// Lamports of an account.
    ///
    /// # Errors
    /// As [`Self::sol`].
    pub async fn sol_balance(&self, account: &str) -> Result<u128> {
        let v = self.sol("getBalance", json!([account])).await?;
        Ok(u128::from(v["value"].as_u64().unwrap_or(0)))
    }

    /// Atomic units of `mint` an owner holds in its token accounts, with
    /// the mint's decimals.
    ///
    /// # Errors
    /// As [`Self::sol`].
    pub async fn sol_token(&self, owner: &str, mint: &str) -> Result<(u128, Option<u8>)> {
        let v = self
            .sol(
                "getTokenAccountsByOwner",
                json!([owner, {"mint": mint}, {"encoding": "jsonParsed"}]),
            )
            .await?;
        let mut total = 0u128;
        let mut decimals = None;
        for account in v["value"].as_array().cloned().unwrap_or_default() {
            let amount = &account["account"]["data"]["parsed"]["info"]["tokenAmount"];
            total += amount["amount"]
                .as_str()
                .and_then(|a| a.parse().ok())
                .unwrap_or(0);
            decimals = amount["decimals"].as_u64().map(|d| d as u8);
        }
        Ok((total, decimals))
    }

    /// Broadcast a signed Solana transaction; returns its signature.
    ///
    /// # Errors
    /// As [`Self::sol`].
    pub async fn sol_send(&self, raw: &[u8]) -> Result<String> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
        let v = self
            .sol("sendTransaction", json!([encoded, {"encoding": "base64"}]))
            .await?;
        v.as_str()
            .map(str::to_string)
            .ok_or_else(|| unavailable("signature"))
    }

    /// `Some(succeeded)` once a transaction is finalized.
    ///
    /// # Errors
    /// As [`Self::sol`].
    pub async fn sol_status(&self, signature: &str) -> Result<Option<bool>> {
        let v = self
            .sol(
                "getSignatureStatuses",
                json!([[signature], {"searchTransactionHistory": true}]),
            )
            .await?;
        let status = &v["value"][0];
        if status.is_null() || status["confirmationStatus"] != "finalized" {
            return Ok(None);
        }
        Ok(Some(status["err"].is_null()))
    }
}

fn quantity(v: &Value) -> Result<u128> {
    let text = v.as_str().ok_or_else(|| unavailable("quantity"))?;
    u128::from_str_radix(text.trim_start_matches("0x"), 16).map_err(unavailable)
}

fn decode_sol(text: &str) -> Result<Hash> {
    dmsg_protocol::wallet::parse_sol_address(text)
        .map(Hash::new)
        .map_err(|_| unavailable("blockhash"))
}
