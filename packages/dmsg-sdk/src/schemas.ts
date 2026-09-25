// Generated exact wire schemas; unknown fields are rejected.
export const schemas = {
  "Eligibility": [
    "Eligible",
    "Ineligible",
    "Unverifiable"
  ],
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
    "action_authority": "Principal",
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
    "effective_at_ms": "u64"
  },
  "PandaQuote": {
    "quoted_at_ms": "u64",
    "version": "u16",
    "offer_hash": "Hash",
    "policy": "PandaRatePolicy",
    "required_stake_e8s": "u128",
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
    "PandaSubscription"
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
    "decided_at_ms": "u64",
    "apply_by_ms": "u64"
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
  },
  "AppAction": {
    "version": "u16",
    "environment": "Environment",
    "app_id": "String",
    "app_config_version": "u64",
    "origin": "String",
    "receiver": "Principal",
    "actor_id": "AccountId",
    "signing_account": "AccountId",
    "operation_id": "Hash",
    "intent_hash": "Hash",
    "input_hash": "Hash",
    "subject_hash": "Hash",
    "precondition_hash": "Hash",
    "role_snapshot_hash": "Hash",
    "signing_policy_hash": "Hash",
    "rule_set_hash": "Hash",
    "issued_at_ms": "u64",
    "expires_at_ms": "u64",
    "command": "AppActionCommand",
    "files": "Vec<ActionFile>"
  },
  "AppActionCommand": [
    {
      "TokenListCertifyDisclosure": {
        "project_id": "u64",
        "contract_id": "u64",
        "revision": "u64"
      }
    },
    {
      "TokenListDecideReview": {
        "project_id": "u64",
        "case_id": "u64",
        "round": "u64",
        "outcome": "ActionReviewOutcome",
        "changes": "Vec<ActionRequestedChange>",
        "rationale": "String"
      }
    },
    {
      "TokenListCertifyTransition": {
        "project_id": "u64",
        "transition_id": "u64",
        "statement_hash": "Hash",
        "rationale": "String",
        "analysis": "Option<ActionArtifact>"
      }
    },
    {
      "TokenListApproveTransition": {
        "project_id": "u64",
        "transition_id": "u64",
        "approve": "bool",
        "statement_hash": "Hash",
        "rationale": "String"
      }
    }
  ],
  "ActionReviewOutcome": [
    "Approved",
    "Rejected",
    "ChangesRequested"
  ],
  "ActionRequestedChange": {
    "locator": "String",
    "detail": "String",
    "blocking": "bool"
  },
  "ActionArtifact": {
    "uri": "String",
    "sha256": "Hash",
    "content_type": "String",
    "size": "u64"
  },
  "ActionFile": {
    "file_id": "String",
    "revision": "u64",
    "sha256": "Hash",
    "byte_length": "u64",
    "media_type": "String",
    "display_name": "Option<String>",
    "representation": "ActionFileRepresentation"
  },
  "ActionFileRepresentation": [
    "Original",
    "Encrypted"
  ],
  "SettlementAssetKind": [
    "CkUsdt",
    "CkUsdc"
  ],
  "SettlementAsset": {
    "version": "u16",
    "policy_version": "u64",
    "environment": "Environment",
    "ledger": "Principal",
    "asset": "SettlementAssetKind",
    "decimals": "u16",
    "price_usd_micros": "u128",
    "price_observed_at_ms": "u64",
    "price_valid_until_ms": "u64",
    "network_fee_atomic": "u128",
    "max_network_fee_atomic": "u128",
    "enabled": "bool"
  },
  "ProductApproval": {
    "version": "u16",
    "approval_id": "Hash",
    "offer_hash": "Hash",
    "operator": "Principal",
    "method": "SettlementMethod",
    "approved_at_ms": "u64",
    "expires_at_ms": "u64"
  },
  "ProductAuthorizationRequest": {
    "offer": "BillingOffer",
    "account_approval": "ApplicationApproval",
    "user_home": "Principal",
    "approval_id": "Hash",
    "product_approval": "Option<ProductApproval>"
  },
  "ProductAuthorization": {
    "request_hash": "Hash",
    "operator": "Principal",
    "verified_at_ms": "u64",
    "valid_until_ms": "u64"
  },
  "CheckoutQuote": {
    "product": "ProductRegistration",
    "offer": "BillingOffer",
    "cash": "CashQuote",
    "asset": "SettlementAsset",
    "quoted_at_ms": "u64"
  },
  "OpenCheckout": {
    "quote": "CheckoutQuote",
    "authorization": "ProductAuthorizationRequest"
  },
  "CheckoutStatus": [
    "Reserving",
    "AwaitingFunding",
    "Applying",
    "Applied",
    "RefundCommitted",
    "Rejected"
  ],
  "CheckoutProgress": {
    "order_id": "Hash",
    "status": "CheckoutStatus",
    "decision_id": "Option<Hash>"
  },
  "CheckoutView": {
    "progress": "CheckoutProgress",
    "quote": "CheckoutQuote",
    "receipt": "Option<ProductReceipt>",
    "outgoing_atomic": "u128",
    "service_reserve_atomic": "u128",
    "fee_reserve_atomic": "u128"
  },
  "CashBlock": {
    "ledger": "Principal",
    "block_index": "u128"
  },
  "CheckoutDeposit": {
    "order_id": "Hash",
    "block": "CashBlock",
    "from": "Account",
    "amount_atomic": "u128",
    "refundable_atomic": "u128"
  },
  "CashTransferStatus": [
    "Pending",
    "InFlight",
    "Unknown",
    "Rejected",
    "Succeeded",
    "Superseded"
  ],
  "CashTransfer": {
    "transfer_id": "Hash",
    "order_id": "Hash",
    "ledger": "Principal",
    "source_subaccount": "Hash",
    "to": "Account",
    "amount_atomic": "u128",
    "fee_atomic": "u128",
    "max_fee_atomic": "u128",
    "memo": "Hash",
    "created_at_time_ns": "u64",
    "status": "CashTransferStatus",
    "block_index": "Option<u128>",
    "expected_fee_atomic": "Option<u128>",
    "error_code": "Option<String>",
    "replaces": "Option<Hash>",
    "replaced_by": "Option<Hash>"
  },
  "SubscriptionSource": [
    "Included",
    {
      "Cash": {
        "order_id": "Hash",
        "ledger": "Principal",
        "block_index": "u128",
        "amount_atomic": "u128"
      }
    },
    {
      "PandaClaim": {
        "claim_id": "Hash",
        "quote": "PandaQuote"
      }
    }
  ],
  "SubscriptionStatus": [
    "Active",
    "Repairing",
    "Unverifiable",
    "Terminated",
    "Expired",
    "Cancelled"
  ],
  "SubscriptionContract": {
    "contract_id": "Hash",
    "offer": "BillingOffer",
    "source": "SubscriptionSource",
    "decision_id": "Option<Hash>",
    "applied_at_ms": "u64",
    "business_revision": "u64",
    "lease_revision": "u64",
    "status": "SubscriptionStatus",
    "lease_until_ms": "u64",
    "max_issued_until_ms": "u64",
    "qualification": "Eligibility",
    "observed_at_ms": "u64",
    "repair_elapsed_ms": "u64"
  },
  "CashCancellationReceipt": {
    "order_id": "Hash",
    "contract_id": "Hash",
    "decision_hash": "Hash",
    "cancelled_at_ms": "u64",
    "cancelled": "bool",
    "business_revision": "u64"
  },
  "CashTransferProgress": {
    "transfer_id": "Hash",
    "status": "CashTransferStatus",
    "block_index": "Option<u128>"
  },
  "CheckoutRequest": {
    "offer": "BillingOffer",
    "approving_account": "AccountId",
    "product_approval": "Option<ProductApproval>",
    "method": "SettlementMethod"
  },
  "SettlementAssetView": {
    "policy": "SettlementAsset",
    "ledger_verified": "bool"
  },
  "CheckoutLedgerBalance": {
    "ledger": "Principal",
    "incoming_atomic": "u128",
    "refundable_atomic": "u128",
    "service_reserve_atomic": "u128",
    "fee_reserve_atomic": "u128",
    "outgoing_atomic": "u128"
  },
  "CheckoutOperationAudit": {
    "order": "CheckoutView",
    "balances": "Vec<CheckoutLedgerBalance>"
  },
  "CheckoutOperationsPage": {
    "orders": "Vec<CheckoutOperationAudit>",
    "next": "Option<Hash>"
  },
  "CashTransfersPage": {
    "transfers": "Vec<CashTransfer>",
    "next": "Option<Hash>"
  },
  "PandaServiceConfig": {
    "commerce_canister": "Principal",
    "max_claims": "u64",
    "hourly_applications": "u64",
    "cooling_ms": "u64"
  },
  "PandaApplicationTerms": {
    "home_membership": "Principal",
    "user_home": "Principal",
    "approving_account": "AccountId",
    "actor": "Principal",
    "sns_governance": "Principal",
    "neuron_id": "Hash",
    "offer": "BillingOffer",
    "quote": "PandaQuote"
  },
  "PandaClaimRequest": {
    "terms": "PandaApplicationTerms",
    "authorization": "ProductAuthorizationRequest"
  },
  "PandaClaimStatus": [
    "Checking",
    "CoolingDown",
    "Applying",
    "Active",
    "Terminated",
    "Cancelled",
    "Rejected",
    "Released"
  ],
  "PandaClaimView": {
    "version": "u16",
    "claim_id": "Hash",
    "terms": "PandaApplicationTerms",
    "status": "PandaClaimStatus",
    "eligibility": "Eligibility",
    "observed_at_ms": "u64",
    "valid_until_ms": "u64",
    "lease_revision": "u64",
    "cooling_until_ms": "Option<u64>",
    "committed_until_ms": "u64",
    "repair_elapsed_ms": "u64",
    "receipt": "Option<ProductReceipt>"
  },
  "PandaOperationsPage": {
    "claims": "Vec<PandaClaimView>",
    "next": "Option<Hash>"
  },
  "KeyPurpose": [
    "AppAction",
    "FileAttestation",
    "Statement",
    "ContentRoot"
  ],
  "Algorithm": [
    "Ed25519",
    "EcdsaSecp256k1",
    "VetKdBls12381"
  ],
  "KeyDescriptor": {
    "key_id": "ByteBuf",
    "account_id": "AccountId",
    "purpose": "KeyPurpose",
    "algorithm": "Algorithm",
    "home_cose": "Principal",
    "master_key_name": "String",
    "environment": "Environment",
    "derivation_version": "u16",
    "key_generation": "u64",
    "public_key": "ByteBuf",
    "public_key_fingerprint": "Hash"
  },
  "ExecutionStatus": [
    "Authorized",
    "Executing",
    "Completed",
    "Failed",
    "Unknown",
    "ResultExpired"
  ],
  "ExecutionReceipt": {
    "schema": "u16",
    "account_id": "AccountId",
    "issuer": "String",
    "request_id": "OpId",
    "device_id": "Hash",
    "security_epoch": "u64",
    "approved_at": "u64",
    "expires_at": "u64",
    "origin": "String",
    "max_cycles": "u128",
    "to_be_signed_digest": "Hash",
    "public_key_fingerprint": "Hash",
    "status": "ExecutionStatus",
    "signature_digest": "Option<Hash>"
  }
} as const
