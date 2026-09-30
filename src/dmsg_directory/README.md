# dmsg_directory

Publishes Agent Delegation 1.0 principal documents for every dMsg user home at one custom domain, as ICP-certified HTTP responses. User homes remain authoritative for controllers and signing; this canister only serves what they publish.

- `publish(account_id, PrincipalState)`: only the account's home publishes (first by the Xid allocator fingerprint, then the recorded home). Versions only increase; the same version must carry the same state.
- `get_publication`, `directory_config`: queries.
- `http_request`: `GET /<account_id>` returns the exact JCS document; other paths return a certified 404; `/.well-known/ic-domains` lists custom domains. Responses are certified response-only with all headers.
- Stable layout schema 1: config (memory 0), documents (memory 1). The heap certification tree holds hashes only and is rebuilt after upgrade by re-certifying stored documents.

Not implemented: home `handoff`, external-key reservation. Capacity (documents per canister, upgrade rebuild instructions) has not been measured. Contract: [docs/protocol/agent_zh.md](../../docs/protocol/agent_zh.md).
