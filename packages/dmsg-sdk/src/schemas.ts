// Generated exact wire schemas; unknown fields are rejected.
export const schemas = {
  "Account": {
    "owner": "Principal",
    "subaccount": "Option<Hash>"
  },
  "Beneficiary": {
    "product_id": "String",
    "authority_canister": "Principal",
    "subject_schema": "String",
    "subject_bytes": "Bytes"
  },
  "Environment": [
    "Local",
    "Staging",
    "Production"
  ],
  "AppCapability": [
    "Authenticate",
    "SignDocument",
    "SignAction",
    "Checkout"
  ],
  "SigningProfile": [
    "TextStatementV1",
    "DigestStatementV1",
    "FileStatementV1",
    "AppActionV1"
  ],
  "AppRegistration": {
    "version": "u16",
    "environment": "Environment",
    "app_id": "String",
    "config_version": "u64",
    "origins": "Vec<String>",
    "user_homes": "Vec<Principal>",
    "cose_homes": "Vec<Principal>",
    "product_ids": "Vec<String>",
    "capabilities": "Vec<AppCapability>",
    "profiles": "Vec<SigningProfile>",
    "authentication_receiver": "Principal",
    "paused": "bool"
  },
  "ProductRegistration": {
    "version": "u16",
    "environment": "Environment",
    "product_id": "String",
    "config_version": "u64",
    "quote_authority": "Principal",
    "beneficiary_authority": "Principal",
    "adapter": "Principal",
    "subject_schema": "String",
    "subject_size": "u16",
    "merchant": "Account",
    "ledgers": "Vec<Principal>",
    "terms_hash": "Hash",
    "subsidy_budget_id": "Hash",
    "paused": "bool"
  },
  "AuthenticationPurpose": [
    "Login",
    "Link",
    "Reauthenticate"
  ],
  "AuthenticationRequest": {
    "version": "u16",
    "environment": "Environment",
    "app_id": "String",
    "app_config_version": "u64",
    "origin": "String",
    "receiver": "Principal",
    "challenge_hash": "Hash",
    "session_key_hash": "Hash",
    "purpose": "AuthenticationPurpose",
    "nonce": "Hash",
    "operation_id": "Hash",
    "issued_at_ms": "u64",
    "expires_at_ms": "u64"
  },
  "AuthenticationResult": {
    "version": "u16",
    "request": "AuthenticationRequest",
    "account_id": "AccountId",
    "home_user": "Principal",
    "security_epoch": "u64",
    "device_id": "Hash",
    "approved_at_ms": "u64",
    "expires_at_ms": "u64"
  },
  "SettlementMethod": [
    "Cash",
    "Panda"
  ],
  "BillingOffer": {
    "version": "u16",
    "environment": "Environment",
    "app_id": "String",
    "product_id": "String",
    "offer_id": "Hash",
    "beneficiary": "Beneficiary",
    "quote_authority": "Principal",
    "adapter": "Principal",
    "sku": "String",
    "product_terms_hash": "Hash",
    "expected_business_revision": "u64",
    "amount_usd_micros": "u128",
    "starts_at_ms": "u64",
    "expires_at_ms": "u64",
    "issued_at_ms": "u64",
    "accept_by_ms": "u64",
    "operation_id": "Hash",
    "allowed_settlement_methods": "Vec<SettlementMethod>"
  },
  "PandaRatePolicy": {
    "version": "u16",
    "policy_version": "u64",
    "environment": "Environment",
    "product_ids": "Vec<String>",
    "r_num": "u128",
    "r_den": "u128",
    "published_at_ms": "u64",
    "effective_at_ms": "u64",
    "subsidy_budget_id": "Hash"
  },
  "PandaQuote": {
    "version": "u16",
    "offer_hash": "Hash",
    "policy": "PandaRatePolicy",
    "required_stake_e8s": "u128",
    "subsidy_usd_micros": "u128",
    "application_deadline_ms": "u64",
    "committed_until_ms": "u64"
  },
  "CashQuote": {
    "version": "u16",
    "offer_hash": "Hash",
    "ledger": "Principal",
    "amount_atomic": "u128",
    "conversion_hash": "Hash",
    "payer": "Account",
    "deposit": "Account",
    "max_network_fee_atomic": "u128",
    "fee_reserve_atomic": "u128",
    "funding_deadline_ms": "u64",
    "activation_deadline_ms": "u64"
  },
  "ApprovalPurpose": [
    "CashCheckout",
    "PandaSubscription",
    "AppAction"
  ],
  "ApplicationApproval": {
    "version": "u16",
    "environment": "Environment",
    "app_id": "String",
    "app_config_version": "u64",
    "origin": "String",
    "approving_account": "AccountId",
    "service": "Principal",
    "beneficiary": "Beneficiary",
    "actor": "Principal",
    "purpose": "ApprovalPurpose",
    "action_digest": "Hash",
    "operation_id": "Hash",
    "nonce": "Hash",
    "expires_at_ms": "u64"
  },
  "SettlementSource": [
    {
      "Cash": {
        "order_id": "Hash",
        "ledger": "Principal",
        "block_index": "u128",
        "amount_atomic": "u128"
      }
    },
    {
      "Panda": {
        "claim_id": "Hash",
        "quote_hash": "Hash",
        "committed_until_ms": "u64",
        "lease_until_ms": "u64"
      }
    }
  ],
  "ProductDecision": {
    "version": "u16",
    "offer": "BillingOffer",
    "decision_id": "Hash",
    "source": "SettlementSource",
    "decided_at_ms": "u64"
  },
  "ProductOutcome": [
    {
      "Applied": {
        "business_revision": "u64",
        "contract_id": "Hash",
        "committed_until_ms": "u64"
      }
    },
    {
      "Rejected": {
        "reason": "ProductRejection"
      }
    }
  ],
  "ProductRejection": [
    "RevisionConflict",
    "Unauthorized",
    "Expired",
    "IntervalReserved",
    "OfferMismatch"
  ],
  "ProductReceipt": {
    "version": "u16",
    "decision_id": "Hash",
    "decision_hash": "Hash",
    "adapter": "Principal",
    "outcome": "ProductOutcome",
    "applied_at_ms": "u64"
  },
  "ApplicationAuthorization": {
    "approval_id": "Hash",
    "approval_hash": "Hash",
    "security_epoch": "u64",
    "verified_at_ms": "u64",
    "valid_until_ms": "u64"
  }
} as const
