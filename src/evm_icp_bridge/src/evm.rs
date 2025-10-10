use alloy::{
    primitives::{hex::FromHex, Address, TxHash, U256},
    rpc::types::TransactionReceipt,
};
use ic_cdk::management_canister::{
    http_request, HttpHeader, HttpMethod, HttpRequestArgs, HttpRequestResult, TransformArgs,
    TransformContext, TransformFunc,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

pub static APP_AGENT: &str = concat!(
    "Mozilla/5.0 ICP canister ",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
);

pub struct EvmClient {
    pub providers: Vec<String>,
    pub api_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RPCRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: &'a [Value],
    id: u64,
}

#[derive(Debug, Deserialize)]
pub struct RPCResponse<T> {
    result: Option<T>,
    error: Option<Value>,
}

impl EvmClient {
    pub async fn chain_id(&self, now_ms: u64) -> Result<u64, String> {
        let res: String = self
            .call(format!("eth_chainId-{}", now_ms), "eth_chainId", &[])
            .await?;
        hex_to_u64(&res)
    }

    pub async fn gas_price(&self, now_ms: u64) -> Result<u128, String> {
        let res: String = self
            .call(format!("eth_gasPrice-{}", now_ms), "eth_gasPrice", &[])
            .await?;
        hex_to_u128(&res)
    }

    pub async fn finalized_block_number(&self, now_ms: u64) -> Result<u64, String> {
        let res: String = self
            .call(
                format!("eth_blockNumber-{}", now_ms),
                "eth_blockNumber",
                &[],
            )
            .await?;
        let num = hex_to_u64(&res)?;
        Ok(num.saturating_sub(42)) // consider a block finalized if it is 42 blocks deep
    }

    pub async fn get_transaction_count(
        &self,
        now_ms: u64,
        address: &Address,
    ) -> Result<u64, String> {
        let res: String = self
            .call(
                format!("eth_getTransactionCount-{}", now_ms),
                "eth_getTransactionCount",
                &[address.to_string().into(), "finalized".into()],
            )
            .await?;
        hex_to_u64(&res)
    }

    pub async fn get_balance(&self, now_ms: u64, address: &Address) -> Result<u128, String> {
        let res: String = self
            .call(
                format!("eth_getBalance-{}", now_ms),
                "eth_getBalance",
                &[address.to_string().into(), "finalized".into()],
            )
            .await?;
        hex_to_u128(&res)
    }

    pub async fn get_transaction_receipt(
        &self,
        now_ms: u64,
        tx_hash: &TxHash,
    ) -> Result<TransactionReceipt, String> {
        self.call(
            format!("eth_getTransactionReceipt-{}", now_ms),
            "eth_getTransactionReceipt",
            &[tx_hash.to_string().into()],
        )
        .await
    }

    pub async fn send_raw_transaction(
        &self,
        idempotency_key: String,
        signed_tx: String,
    ) -> Result<TxHash, String> {
        self.call(
            idempotency_key,
            "eth_sendRawTransaction",
            &[signed_tx.into()],
        )
        .await
    }

    pub async fn call_contract(
        &self,
        now_ms: u64,
        contract: &Address,
        call_data: String,
    ) -> Result<Vec<u8>, String> {
        let call_object = serde_json::json!({
            "to": contract.to_string(),
            "data": call_data,
        });

        let res: String = self
            .call(
                format!("eth_call-{}", now_ms),
                "eth_call",
                &[call_object, "latest".into()],
            )
            .await?;
        let res = res.strip_prefix("0x").unwrap_or(&res);
        <Vec<u8>>::from_hex(res).map_err(|err| err.to_string())
    }

    pub async fn erc20_name(&self, now_ms: u64, contract: &Address) -> Result<String, String> {
        let res = self
            .call_contract(now_ms, contract, "0x06fdde03".to_string())
            .await?;
        decode_abi_string(&res)
    }

    pub async fn erc20_symbol(&self, now_ms: u64, contract: &Address) -> Result<String, String> {
        let res = self
            .call_contract(now_ms, contract, "0x95d89b41".to_string())
            .await?;
        decode_abi_string(&res)
    }

    pub async fn erc20_decimals(&self, now_ms: u64, contract: &Address) -> Result<u8, String> {
        let res = self
            .call_contract(now_ms, contract, "0x313ce567".to_string())
            .await?;
        let v = decode_abi_uint(&res)?;
        u8::try_from(v).map_err(|_| "decimals overflow u8".to_string())
    }

    pub async fn call<T: DeserializeOwned>(
        &self,
        idempotency_key: String,
        method: &str,
        params: &[Value],
    ) -> Result<T, String> {
        if self.providers.is_empty() {
            return Err("no available provider".to_string());
        }

        let input = RPCRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };
        let input = serde_json::to_vec(&input).map_err(|err| err.to_string())?;
        let data = self.http_request(idempotency_key, input).await?;

        let output: RPCResponse<T> =
            serde_json::from_slice(&data).map_err(|err| err.to_string())?;

        if let Some(error) = output.error {
            return Err(serde_json::to_string(&error).map_err(|err| err.to_string())?);
        }

        match output.result {
            Some(result) => Ok(result),
            None => serde_json::from_value(Value::Null).map_err(|err| err.to_string()),
        }
    }

    async fn http_request(
        &self,
        idempotency_key: String,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        let mut request_headers = vec![
            HttpHeader {
                name: "content-type".to_string(),
                value: "application/json".to_string(),
            },
            HttpHeader {
                name: "user-agent".to_string(),
                value: APP_AGENT.to_string(),
            },
            HttpHeader {
                name: "idempotency-key".to_string(),
                value: idempotency_key.clone(),
            },
        ];

        if let Some(api_token) = &self.api_token {
            request_headers.push(HttpHeader {
                name: "authorization".to_string(),
                value: api_token.clone(),
            });
        }

        let mut request = HttpRequestArgs {
            url: "".to_string(),
            max_response_bytes: None, //optional for request
            method: HttpMethod::POST,
            headers: request_headers,
            body: Some(body),
            transform: Some(TransformContext {
                function: TransformFunc::new(
                    ic_cdk::api::canister_self(),
                    "inner_transform_response".to_string(),
                ),
                context: vec![],
            }),
        };

        let mut last_err = String::new();
        for p in &self.providers {
            request.url = p.clone();
            let res = http_request(&request).await;
            match res {
                Ok(res) => {
                    if res.status >= 200u64 && res.status < 300u64 {
                        return Ok(res.body);
                    } else {
                        last_err = format!(
                            "request provider: {}, idempotency-key: {}, status: {}, body: {}",
                            p,
                            idempotency_key,
                            res.status,
                            String::from_utf8(res.body.clone()).unwrap_or_default(),
                        );
                    }
                }
                Err(err) => {
                    return Err(format!("failed to request provider: {}, error: {err}", p,));
                }
            }
        }

        Err(last_err)
    }
}

#[ic_cdk::query(hidden = true)]
fn inner_transform_response(args: TransformArgs) -> HttpRequestResult {
    HttpRequestResult {
        status: args.response.status,
        body: args.response.body,
        // Remove headers (which may contain a timestamp) for consensus
        headers: vec![],
    }
}

pub fn encode_erc20_transfer(to: &Address, value: u128) -> Vec<u8> {
    const TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb]; // keccak256("transfer(address,uint256)")[:4]

    let mut call_data = Vec::with_capacity(4 + 32 + 32);
    call_data.extend_from_slice(&TRANSFER_SELECTOR);

    let mut padded_to = [0u8; 32];
    padded_to[12..].copy_from_slice(to.as_slice());
    call_data.extend_from_slice(&padded_to);

    let value_bytes = U256::from(value).to_be_bytes::<32>();
    call_data.extend_from_slice(&value_bytes);

    call_data
}

fn hex_to_u64(s: &str) -> Result<u64, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(s, 16).map_err(|err| err.to_string())
}

fn hex_to_u128(s: &str) -> Result<u128, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u128::from_str_radix(s, 16).map_err(|err| err.to_string())
}

fn decode_abi_string(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() < 64 {
        return Err("abi string result too short".to_string());
    }

    let offset = U256::try_from_be_slice(&bytes[0..32]).unwrap();
    let offset = usize::try_from(offset).unwrap_or(isize::MAX as usize);
    if bytes.len() < offset + 32 {
        return Err("abi string length out of bounds".to_string());
    }

    let len = U256::try_from_be_slice(&bytes[offset..offset + 32]).unwrap();
    let len = usize::try_from(len).unwrap_or(isize::MAX as usize);
    if bytes.len() < offset + 32 + len {
        return Err("abi string data out of bounds".to_string());
    }

    let data = &bytes[offset + 32..offset + 32 + len];
    String::from_utf8(data.to_vec()).map_err(|err| err.to_string())
}

fn decode_abi_uint(bytes: &[u8]) -> Result<U256, String> {
    if bytes.len() != 32 {
        return Err("abi uint result must be 32 bytes".to_string());
    }
    Ok(U256::from_be_slice(bytes))
}
