A self-signed certificate for `localhost` and `127.0.0.1` (valid until 2126)
and its PKCS#8 private key, both DER. They are public test material, copied
from the agent-protocols repository's `crates/agent-protocols/tests/fixtures/knowledge-tls`.
Never use this key for a real service.

Tests serve local HTTPS Agent Protocols hosts with them — Agent Mail and
Agent Knowledge accept only HTTPS origins — and trust the certificate
through the local-only `agent_ca` setting.
