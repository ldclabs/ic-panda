// Generated from the public dmsg_user.did. Run npm run bindings.
export const idlFactory = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const UserInit = IDL.Record({
    'handle_canister' : IDL.Principal,
    'principal_origin' : IDL.Text,
    'home_cose' : IDL.Principal,
    'issuer_namespace' : IDL.Text,
    'daily_new_accounts' : IDL.Nat32,
    'payment_canister' : IDL.Principal,
    'environment' : Environment,
    'commerce_canister' : IDL.Principal,
    'governance' : IDL.Principal,
    'max_accounts' : IDL.Nat64,
    'membership_canister' : IDL.Principal,
    'directory_canister' : IDL.Principal,
  });
  const Error = IDL.Variant({
    'MigrationKeyUnavailable' : IDL.Null,
    'LegacyWriteDisabled' : IDL.Null,
    'InvalidInput' : IDL.Text,
    'IntervalReserved' : IDL.Null,
    'NeuronOccupied' : IDL.Null,
    'RekeyRequired' : IDL.Null,
    'VersionConflict' : IDL.Null,
    'ExecutionUnknown' : IDL.Null,
    'IntegrityFailed' : IDL.Null,
    'IdTimestampOutOfRange' : IDL.Null,
    'NotFound' : IDL.Null,
    'FeeBlocked' : IDL.Null,
    'DeviceNotApproved' : IDL.Null,
    'Locked' : IDL.Null,
    'MembershipClosing' : IDL.Null,
    'RecoveryIncomplete' : IDL.Null,
    'IdCapacityExceeded' : IDL.Null,
    'PolicyStale' : IDL.Null,
    'IdGeneratorStateConflict' : IDL.Null,
    'IdempotencyConflict' : IDL.Null,
    'UnsupportedProtocol' : IDL.Null,
    'Unavailable' : IDL.Text,
    'MembershipStale' : IDL.Null,
    'Forbidden' : IDL.Null,
    'ResultExpired' : IDL.Null,
    'Expired' : IDL.Null,
    'MembershipIneligible' : IDL.Null,
    'QuotaExceeded' : IDL.Null,
    'AuthRequired' : IDL.Null,
    'Pending' : IDL.Null,
  });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : Error });
  const Beneficiary = IDL.Record({
    'product_id' : IDL.Text,
    'authority_canister' : IDL.Principal,
    'subject_bytes' : IDL.Vec(IDL.Nat8),
    'subject_schema' : IDL.Text,
  });
  const ApprovalPurpose = IDL.Variant({
    'CashCheckout' : IDL.Null,
    'PandaSubscription' : IDL.Null,
  });
  const ApplicationApproval = IDL.Record({
    'service' : IDL.Principal,
    'actor' : IDL.Principal,
    'beneficiary' : Beneficiary,
    'app_config_version' : IDL.Nat64,
    'origin' : IDL.Text,
    'action_digest' : IDL.Vec(IDL.Nat8),
    'operation_id' : IDL.Vec(IDL.Nat8),
    'approving_account' : IDL.Vec(IDL.Nat8),
    'version' : IDL.Nat16,
    'app_id' : IDL.Text,
    'nonce' : IDL.Vec(IDL.Nat8),
    'environment' : Environment,
    'purpose' : ApprovalPurpose,
    'expires_at_ms' : IDL.Nat64,
  });
  const Approval = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'signature' : IDL.Vec(IDL.Nat8),
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'expires_at' : IDL.Nat64,
    'sequence' : IDL.Nat64,
  });
  const Result_1 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : Error });
  const AuthenticationPurpose = IDL.Variant({
    'Reauthenticate' : IDL.Null,
    'Login' : IDL.Null,
    'Link' : IDL.Null,
  });
  const AuthenticationRequest = IDL.Record({
    'app_config_version' : IDL.Nat64,
    'origin' : IDL.Text,
    'operation_id' : IDL.Vec(IDL.Nat8),
    'session_key_hash' : IDL.Vec(IDL.Nat8),
    'version' : IDL.Nat16,
    'app_id' : IDL.Text,
    'issued_at_ms' : IDL.Nat64,
    'nonce' : IDL.Vec(IDL.Nat8),
    'environment' : Environment,
    'receiver' : IDL.Principal,
    'purpose' : AuthenticationPurpose,
    'expires_at_ms' : IDL.Nat64,
    'challenge_hash' : IDL.Vec(IDL.Nat8),
  });
  const AuthenticationResult = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'request' : AuthenticationRequest,
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'version' : IDL.Nat16,
    'home_user' : IDL.Principal,
    'expires_at_ms' : IDL.Nat64,
    'approved_at_ms' : IDL.Nat64,
  });
  const Result_2 = IDL.Variant({ 'Ok' : AuthenticationResult, 'Err' : Error });
  const ActionFileRepresentation = IDL.Variant({
    'Encrypted' : IDL.Null,
    'Original' : IDL.Null,
  });
  const ActionFile = IDL.Record({
    'sha256' : IDL.Vec(IDL.Nat8),
    'media_type' : IDL.Text,
    'byte_length' : IDL.Nat64,
    'display_name' : IDL.Opt(IDL.Text),
    'revision' : IDL.Nat64,
    'representation' : ActionFileRepresentation,
    'file_id' : IDL.Text,
  });
  const ActionArtifact = IDL.Record({
    'uri' : IDL.Text,
    'sha256' : IDL.Vec(IDL.Nat8),
    'size' : IDL.Nat64,
    'content_type' : IDL.Text,
  });
  const ActionRequestedChange = IDL.Record({
    'blocking' : IDL.Bool,
    'locator' : IDL.Text,
    'detail' : IDL.Text,
  });
  const ActionReviewOutcome = IDL.Variant({
    'Approved' : IDL.Null,
    'Rejected' : IDL.Null,
    'ChangesRequested' : IDL.Null,
  });
  const AppActionCommand = IDL.Variant({
    'TokenListCertifyTransition' : IDL.Record({
      'transition_id' : IDL.Nat64,
      'rationale' : IDL.Text,
      'project_id' : IDL.Nat64,
      'analysis' : IDL.Opt(ActionArtifact),
      'statement_hash' : IDL.Vec(IDL.Nat8),
    }),
    'TokenListCertifyDisclosure' : IDL.Record({
      'contract_id' : IDL.Nat64,
      'project_id' : IDL.Nat64,
      'revision' : IDL.Nat64,
    }),
    'TokenListApproveTransition' : IDL.Record({
      'approve' : IDL.Bool,
      'transition_id' : IDL.Nat64,
      'rationale' : IDL.Text,
      'project_id' : IDL.Nat64,
      'statement_hash' : IDL.Vec(IDL.Nat8),
    }),
    'TokenListDecideReview' : IDL.Record({
      'case_id' : IDL.Nat64,
      'rationale' : IDL.Text,
      'project_id' : IDL.Nat64,
      'changes' : IDL.Vec(ActionRequestedChange),
      'outcome' : ActionReviewOutcome,
      'round' : IDL.Nat64,
    }),
  });
  const AppAction = IDL.Record({
    'files' : IDL.Vec(ActionFile),
    'rule_set_hash' : IDL.Vec(IDL.Nat8),
    'app_config_version' : IDL.Nat64,
    'origin' : IDL.Text,
    'signing_account' : IDL.Vec(IDL.Nat8),
    'actor_id' : IDL.Vec(IDL.Nat8),
    'operation_id' : IDL.Vec(IDL.Nat8),
    'subject_hash' : IDL.Vec(IDL.Nat8),
    'version' : IDL.Nat16,
    'command' : AppActionCommand,
    'app_id' : IDL.Text,
    'issued_at_ms' : IDL.Nat64,
    'environment' : Environment,
    'precondition_hash' : IDL.Vec(IDL.Nat8),
    'intent_hash' : IDL.Vec(IDL.Nat8),
    'receiver' : IDL.Principal,
    'input_hash' : IDL.Vec(IDL.Nat8),
    'expires_at_ms' : IDL.Nat64,
    'role_snapshot_hash' : IDL.Vec(IDL.Nat8),
    'signing_policy_hash' : IDL.Vec(IDL.Nat8),
  });
  const StatementContent = IDL.Variant({
    'AppAction' : AppAction,
    'FileStatement' : IDL.Record({
      'sha256' : IDL.Vec(IDL.Nat8),
      'text' : IDL.Text,
      'content_type' : IDL.Opt(IDL.Text),
      'location' : IDL.Opt(IDL.Text),
    }),
    'Text' : IDL.Text,
    'Digest' : IDL.Record({
      'sha256' : IDL.Vec(IDL.Nat8),
      'content_type' : IDL.Opt(IDL.Text),
      'location' : IDL.Opt(IDL.Text),
    }),
  });
  const Statement = IDL.Record({
    'issued_at' : IDL.Opt(IDL.Int64),
    'content' : StatementContent,
    'subject' : IDL.Opt(IDL.Text),
    'issuer' : IDL.Text,
  });
  const AttestRequest = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'signature' : IDL.Vec(IDL.Nat8),
    'statement' : Statement,
    'origin' : IDL.Text,
    'approval' : Approval,
  });
  const SignedArtifact = IDL.Record({
    'cose_sign1' : IDL.Vec(IDL.Nat8),
    'cose_key' : IDL.Vec(IDL.Nat8),
  });
  const Result_3 = IDL.Variant({ 'Ok' : SignedArtifact, 'Err' : Error });
  const AppActionAttestRequest = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'signature' : IDL.Vec(IDL.Nat8),
    'action' : AppAction,
    'issuer' : IDL.Text,
    'approval' : Approval,
  });
  const CertifiedEntry = IDL.Record({
    'key' : IDL.Vec(IDL.Nat8),
    'value' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'witness' : IDL.Vec(IDL.Nat8),
  });
  const CertifiedBatch = IDL.Record({
    'certificate' : IDL.Vec(IDL.Nat8),
    'schema' : IDL.Nat16,
    'entries' : IDL.Vec(CertifiedEntry),
    'canister' : IDL.Principal,
  });
  const Result_4 = IDL.Variant({ 'Ok' : CertifiedBatch, 'Err' : Error });
  const SettlementMethod = IDL.Variant({
    'Cash' : IDL.Null,
    'Panda' : IDL.Null,
  });
  const ProductApproval = IDL.Record({
    'method' : SettlementMethod,
    'approval_id' : IDL.Vec(IDL.Nat8),
    'offer_hash' : IDL.Vec(IDL.Nat8),
    'operator' : IDL.Principal,
    'version' : IDL.Nat16,
    'expires_at_ms' : IDL.Nat64,
    'approved_at_ms' : IDL.Nat64,
  });
  const BillingOffer = IDL.Record({
    'sku' : IDL.Text,
    'product_id' : IDL.Text,
    'beneficiary' : Beneficiary,
    'amount_usd_micros' : IDL.Nat,
    'operation_id' : IDL.Vec(IDL.Nat8),
    'starts_at_ms' : IDL.Nat64,
    'version' : IDL.Nat16,
    'app_id' : IDL.Text,
    'issued_at_ms' : IDL.Nat64,
    'offer_id' : IDL.Vec(IDL.Nat8),
    'quote_authority' : IDL.Principal,
    'environment' : Environment,
    'expected_business_revision' : IDL.Nat64,
    'adapter' : IDL.Principal,
    'allowed_settlement_methods' : IDL.Vec(SettlementMethod),
    'expires_at_ms' : IDL.Nat64,
    'accept_by_ms' : IDL.Nat64,
    'product_terms_hash' : IDL.Vec(IDL.Nat8),
  });
  const ProductAuthorizationRequest = IDL.Record({
    'product_approval' : IDL.Opt(ProductApproval),
    'account_approval' : ApplicationApproval,
    'approval_id' : IDL.Vec(IDL.Nat8),
    'user_home' : IDL.Principal,
    'offer' : BillingOffer,
  });
  const ProductAuthorization = IDL.Record({
    'request_hash' : IDL.Vec(IDL.Nat8),
    'operator' : IDL.Principal,
    'valid_until_ms' : IDL.Nat64,
    'verified_at_ms' : IDL.Nat64,
  });
  const Result_5 = IDL.Variant({ 'Ok' : ProductAuthorization, 'Err' : Error });
  const HandleAction = IDL.Variant({
    'AcceptTransfer' : IDL.Null,
    'Register' : IDL.Null,
    'Transfer' : IDL.Null,
    'ClaimLegacy' : IDL.Null,
  });
  const HandleIntent = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'handle_canister' : IDL.Principal,
    'target_account' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'action' : HandleAction,
    'op_id' : IDL.Vec(IDL.Nat8),
    'handle' : IDL.Text,
    'expected_version' : IDL.Nat64,
    'terms_digest' : IDL.Vec(IDL.Nat8),
  });
  const Capability = IDL.Variant({
    'FormalApprove' : IDL.Null,
    'ContentSign' : IDL.Null,
    'VaultUnlock' : IDL.Null,
    'RootManage' : IDL.Null,
    'PaymentOffer' : IDL.Null,
  });
  const ControllerRole = IDL.Variant({
    'Administrator' : IDL.Null,
    'Member' : IDL.Null,
  });
  const DeviceInput = IDL.Record({
    'capabilities' : IDL.Vec(Capability),
    'role' : ControllerRole,
    'device_id' : IDL.Vec(IDL.Nat8),
    'hpke_pub' : IDL.Vec(IDL.Nat8),
    'signing_pub' : IDL.Vec(IDL.Nat8),
  });
  const CreateAccount = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'device' : DeviceInput,
    'proof' : IDL.Vec(IDL.Nat8),
    'expires_at' : IDL.Nat64,
  });
  const Result_6 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : Error });
  const DeriveRootRequest = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'generation' : IDL.Nat64,
    'max_cycles' : IDL.Nat,
    'approval' : Approval,
    'transport_public_key' : IDL.Vec(IDL.Nat8),
  });
  const KeyDescriptor = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'key_generation' : IDL.Nat64,
    'public_key_fingerprint' : IDL.Vec(IDL.Nat8),
    'derivation_version' : IDL.Nat16,
    'public_key' : IDL.Vec(IDL.Nat8),
    'home_cose' : IDL.Principal,
    'environment' : Environment,
    'master_key_name' : IDL.Text,
  });
  const EncryptedRootKey = IDL.Record({
    'key' : KeyDescriptor,
    'encrypted_key' : IDL.Vec(IDL.Nat8),
  });
  const ExecutionOutcome = IDL.Variant({
    'Failed' : Error,
    'Executing' : IDL.Null,
    'Authorized' : IDL.Null,
    'Unknown' : Error,
    'ResultExpired' : IDL.Null,
    'Completed' : EncryptedRootKey,
  });
  const ExecutionResult = IDL.Record({
    'request_id' : IDL.Vec(IDL.Nat8),
    'cycles_cost_upper_bound' : IDL.Nat,
    'cycles_charged' : IDL.Nat,
    'outcome' : ExecutionOutcome,
  });
  const Result_7 = IDL.Variant({ 'Ok' : ExecutionResult, 'Err' : Error });
  const RootReservation = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'generation' : IDL.Nat64,
    'security_epoch' : IDL.Nat64,
    'expected_generation' : IDL.Nat64,
    'expires_at' : IDL.Nat64,
  });
  const RecoveryRequest = IDL.Record({
    'op_id' : IDL.Vec(IDL.Nat8),
    'device' : DeviceInput,
    'new_auth' : IDL.Principal,
    'expires_at' : IDL.Nat64,
  });
  const PendingRecovery = IDL.Record({
    'request' : RecoveryRequest,
    'execute_after' : IDL.Nat64,
  });
  const KeyPurpose = IDL.Variant({
    'AppAction' : IDL.Null,
    'FileAttestation' : IDL.Null,
    'Statement' : IDL.Null,
  });
  const SensitivePolicy = IDL.Record({
    'daily_executions' : IDL.Nat32,
    'frozen' : IDL.Bool,
    'allowed_purposes' : IDL.Vec(KeyPurpose),
  });
  const ContentRootRef = IDL.Record({
    'body_digest' : IDL.Vec(IDL.Nat8),
    'generation' : IDL.Nat64,
    'suite' : IDL.Text,
    'recipients_digest' : IDL.Vec(IDL.Nat8),
    'bundle_digest' : IDL.Vec(IDL.Nat8),
  });
  const Device = IDL.Record({
    'added_at' : IDL.Nat64,
    'added_by' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'revoked_at' : IDL.Opt(IDL.Nat64),
    'next_sequence' : IDL.Nat64,
    'input' : DeviceInput,
  });
  const VaultWriteState = IDL.Variant({
    'RekeyRequired' : IDL.Null,
    'Ready' : IDL.Null,
    'Uninitialized' : IDL.Null,
  });
  const AccountInfo = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'account_version' : IDL.Nat64,
    'auth_bindings' : IDL.Vec(IDL.Principal),
    'root_slot' : IDL.Opt(RootReservation),
    'recovered_device' : IDL.Opt(IDL.Tuple(IDL.Vec(IDL.Nat8), IDL.Nat64)),
    'security_epoch' : IDL.Nat64,
    'created_at_ms' : IDL.Nat64,
    'pending_recovery' : IDL.Opt(PendingRecovery),
    'issuer' : IDL.Text,
    'home_cose' : IDL.Principal,
    'home_user' : IDL.Principal,
    'recovery_delay_ms' : IDL.Nat64,
    'sensitive_policy' : SensitivePolicy,
    'current_root' : IDL.Opt(ContentRootRef),
    'devices' : IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), Device)),
    'vault_write_state' : VaultWriteState,
  });
  const Result_8 = IDL.Variant({ 'Ok' : AccountInfo, 'Err' : Error });
  const SecuritySnapshot = IDL.Record({
    'content_root_generation' : IDL.Nat64,
    'account_id' : IDL.Vec(IDL.Nat8),
    'account_version' : IDL.Nat64,
    'content_root_digest' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'devices_root' : IDL.Vec(IDL.Nat8),
    'principal_updated_at' : IDL.Opt(IDL.Nat64),
    'schema' : IDL.Nat16,
    'security_epoch' : IDL.Nat64,
    'issuer' : IDL.Text,
    'home_cose' : IDL.Principal,
    'home_user' : IDL.Principal,
    'recovery_delay_ms' : IDL.Nat64,
    'vault_write_state' : VaultWriteState,
    'pending_recovery_digest' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_9 = IDL.Variant({
    'Ok' : IDL.Tuple(
      SecuritySnapshot,
      IDL.Vec(IDL.Tuple(IDL.Vec(IDL.Nat8), Device)),
    ),
    'Err' : Error,
  });
  const ExecutionUsage = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'business_revision' : IDL.Nat64,
    'valid_until_ms' : IDL.Nat64,
    'held_units' : IDL.Nat64,
    'lease_revision' : IDL.Nat64,
    'allowed_units' : IDL.Nat64,
    'charged_units' : IDL.Nat64,
    'weight_policy_version' : IDL.Nat64,
    'month_revision' : IDL.Nat64,
    'month_utc' : IDL.Nat32,
  });
  const Result_10 = IDL.Variant({ 'Ok' : ExecutionUsage, 'Err' : Error });
  const OperationReceipt = IDL.Record({
    'id' : IDL.Vec(IDL.Nat8),
    'account_version' : IDL.Nat64,
    'digest' : IDL.Vec(IDL.Nat8),
  });
  const Result_11 = IDL.Variant({ 'Ok' : OperationReceipt, 'Err' : Error });
  const DelegationAuthority = IDL.Variant({
    'Restricted' : IDL.Record({
      'scopes' : IDL.Vec(IDL.Text),
      'audiences' : IDL.Vec(IDL.Text),
    }),
    'Unrestricted' : IDL.Null,
  });
  const HostedController = IDL.Record({
    'invalid_from' : IDL.Opt(IDL.Nat64),
    'public_key' : IDL.Vec(IDL.Nat8),
    'delegation' : DelegationAuthority,
    'supersedes' : IDL.Vec(IDL.Nat32),
    'name' : IDL.Opt(IDL.Text),
    'generation' : IDL.Nat32,
    'valid_from' : IDL.Nat64,
    'retired_at' : IDL.Opt(IDL.Nat64),
  });
  const PrincipalType = IDL.Variant({
    'Team' : IDL.Null,
    'Person' : IDL.Null,
    'Organization' : IDL.Null,
    'Project' : IDL.Null,
    'Other' : IDL.Null,
  });
  const PrincipalState = IDL.Record({
    'updated_at' : IDL.Nat64,
    'controllers' : IDL.Vec(HostedController),
    'principal_type' : PrincipalType,
    'version' : IDL.Nat64,
  });
  const PrincipalInfo = IDL.Record({
    'state' : PrincipalState,
    'principal_id' : IDL.Text,
    'published_version' : IDL.Nat64,
  });
  const Result_12 = IDL.Variant({ 'Ok' : PrincipalInfo, 'Err' : Error });
  const Result_13 = IDL.Variant({
    'Ok' : IDL.Opt(PendingRecovery),
    'Err' : Error,
  });
  const Result_14 = IDL.Variant({
    'Ok' : IDL.Opt(ContentRootRef),
    'Err' : Error,
  });
  const AccountCommand = IDL.Variant({
    'SetDeviceCapabilities' : IDL.Record({
      'capabilities' : IDL.Vec(Capability),
      'device_id' : IDL.Vec(IDL.Nat8),
    }),
    'RegisterController' : IDL.Record({
      'public_key' : IDL.Vec(IDL.Nat8),
      'delegation' : DelegationAuthority,
      'supersedes' : IDL.Vec(IDL.Nat32),
      'name' : IDL.Opt(IDL.Text),
      'generation' : IDL.Nat32,
      'proof' : IDL.Vec(IDL.Nat8),
    }),
    'SetRecoveryDelay' : IDL.Record({ 'delay_ms' : IDL.Nat64 }),
    'DisputeRecovery' : IDL.Record({ 'op_id' : IDL.Vec(IDL.Nat8) }),
    'EnablePrincipal' : IDL.Record({ 'principal_type' : PrincipalType }),
    'RenameController' : IDL.Record({
      'name' : IDL.Opt(IDL.Text),
      'generation' : IDL.Nat32,
    }),
    'RetireController' : IDL.Record({ 'generation' : IDL.Nat32 }),
    'BindAuth' : IDL.Record({
      'principal' : IDL.Principal,
      'nonce' : IDL.Vec(IDL.Nat8),
    }),
    'ReserveRoot' : IDL.Record({
      'op_id' : IDL.Vec(IDL.Nat8),
      'expected_generation' : IDL.Nat64,
    }),
    'RemoveAuth' : IDL.Record({ 'principal' : IDL.Principal }),
    'AuthorizeHandle' : IDL.Record({ 'intent' : HandleIntent }),
    'AddDevice' : IDL.Record({
      'device' : DeviceInput,
      'proof' : IDL.Vec(IDL.Nat8),
    }),
    'CommitRoot' : IDL.Record({
      'op_id' : IDL.Vec(IDL.Nat8),
      'root' : ContentRootRef,
      'expected_generation' : IDL.Nat64,
    }),
    'MarkControllerCompromised' : IDL.Record({
      'invalid_from' : IDL.Nat64,
      'generation' : IDL.Nat32,
    }),
    'RevokeDevice' : IDL.Record({ 'device_id' : IDL.Vec(IDL.Nat8) }),
    'SetPolicy' : IDL.Record({ 'policy' : SensitivePolicy }),
  });
  const AccountMutation = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'command' : AccountCommand,
    'approval' : Approval,
    'expected_version' : IDL.Nat64,
  });
  const Result_15 = IDL.Variant({ 'Ok' : IDL.Nat32, 'Err' : Error });
  const Result_16 = IDL.Variant({ 'Ok' : IDL.Nat64, 'Err' : Error });
  const UserStats = IDL.Record({
    'day' : IDL.Nat64,
    'unlock_ready' : IDL.Bool,
    'created_today' : IDL.Nat32,
    'cycles' : IDL.Nat,
    'accounts' : IDL.Nat64,
    'daily_new_accounts' : IDL.Nat32,
    'max_accounts' : IDL.Nat64,
    'stable_pages' : IDL.Nat64,
  });
  const Result_17 = IDL.Variant({ 'Ok' : IDL.Text, 'Err' : IDL.Text });
  const ApplicationAuthorization = IDL.Record({
    'approval_id' : IDL.Vec(IDL.Nat8),
    'valid_until_ms' : IDL.Nat64,
    'security_epoch' : IDL.Nat64,
    'verified_at_ms' : IDL.Nat64,
    'approval_hash' : IDL.Vec(IDL.Nat8),
  });
  const Result_18 = IDL.Variant({
    'Ok' : ApplicationAuthorization,
    'Err' : Error,
  });
  const Account = IDL.Record({
    'owner' : IDL.Principal,
    'subaccount' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const PaymentOffer = IDL.Record({
    'account_id' : IDL.Vec(IDL.Nat8),
    'quote_scope' : IDL.Vec(IDL.Nat8),
    'issued_at' : IDL.Nat64,
    'recipient' : Account,
    'device_id' : IDL.Vec(IDL.Nat8),
    'security_epoch' : IDL.Nat64,
    'version' : IDL.Nat64,
    'ledger' : IDL.Principal,
    'offer_id' : IDL.Vec(IDL.Nat8),
    'home_payment' : IDL.Principal,
    'expires_at' : IDL.Nat64,
    'recipient_net' : IDL.Nat,
  });
  const SignedOffer = IDL.Record({
    'signature' : IDL.Vec(IDL.Nat8),
    'offer' : PaymentOffer,
  });
  return IDL.Service({
    'admin_set_account_limits' : IDL.Func([IDL.Nat64, IDL.Nat32], [Result], []),
    'approve_application' : IDL.Func(
        [ApplicationApproval, Approval],
        [Result_1],
        [],
      ),
    'approve_authentication' : IDL.Func(
        [IDL.Vec(IDL.Nat8), AuthenticationRequest, Approval],
        [Result_2],
        [],
      ),
    'attest' : IDL.Func([AttestRequest], [Result_3], []),
    'attest_app_action' : IDL.Func([AppActionAttestRequest], [Result_3], []),
    'authentication_certificate' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_4],
        ['query'],
      ),
    'authorize_product_billing' : IDL.Func(
        [ProductAuthorizationRequest],
        [Result_5],
        [],
      ),
    'begin_auth_binding' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8), IDL.Nat64],
        [Result],
        [],
      ),
    'complete_recovery' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result],
        [],
      ),
    'consume_handle_authorization' : IDL.Func([HandleIntent], [Result], []),
    'consume_handle_transfer_authorizations' : IDL.Func(
        [HandleIntent, HandleIntent],
        [Result],
        [],
      ),
    'create_account' : IDL.Func([CreateAccount], [Result_6], []),
    'derive_root' : IDL.Func([DeriveRootRequest], [Result_7], []),
    'get_account' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_8], ['query']),
    'get_attestation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_3],
        ['query'],
      ),
    'get_device_bundle' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_9], ['query']),
    'get_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_7],
        ['query'],
      ),
    'get_execution_receipt' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_4],
        ['query'],
      ),
    'get_execution_usage' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat32],
        [Result_10],
        ['query'],
      ),
    'get_execution_usage_certified' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Nat32],
        [Result_4],
        ['query'],
      ),
    'get_operation' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_11],
        ['query'],
      ),
    'get_principal' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_12], ['query']),
    'get_recovery_request' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_13],
        ['query'],
      ),
    'get_root_ref' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_14], ['query']),
    'inspect_app_action' : IDL.Func(
        [IDL.Vec(IDL.Nat8), AppAction],
        [Result],
        [],
      ),
    'mutate_account' : IDL.Func([AccountMutation], [Result_11], []),
    'my_account' : IDL.Func([], [IDL.Opt(IDL.Vec(IDL.Nat8))], ['query']),
    'prune_auth_bindings' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [],
      ),
    'prune_executions' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_15], []),
    'prune_external_approvals' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [IDL.Opt(IDL.Vec(IDL.Nat8))],
        [],
      ),
    'publish_principal' : IDL.Func([IDL.Vec(IDL.Nat8)], [Result_16], []),
    'reconcile_execution' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_7],
        [],
      ),
    'refresh_execution_entitlement' : IDL.Func(
        [IDL.Vec(IDL.Nat8)],
        [Result_10],
        [],
      ),
    'request_recovery' : IDL.Func(
        [IDL.Vec(IDL.Nat8), RecoveryRequest, IDL.Vec(IDL.Nat8)],
        [Result],
        [],
      ),
    'security_snapshot_batch' : IDL.Func(
        [IDL.Vec(IDL.Vec(IDL.Nat8))],
        [Result_4],
        ['query'],
      ),
    'unlock_secret' : IDL.Func(
        [IDL.Vec(IDL.Nat8), IDL.Vec(IDL.Nat8)],
        [Result_6],
        ['query'],
      ),
    'user_stats' : IDL.Func([], [UserStats], ['query']),
    'validate_admin_set_account_limits' : IDL.Func(
        [IDL.Nat64, IDL.Nat32],
        [Result_17],
        ['query'],
      ),
    'verify_application_authorization' : IDL.Func(
        [IDL.Vec(IDL.Nat8), ApplicationApproval],
        [Result_18],
        [],
      ),
    'verify_payment_offer' : IDL.Func([SignedOffer], [Result_16], []),
    'verify_product_account' : IDL.Func([IDL.Text, Beneficiary], [Result], []),
  });
};
export const init = ({ IDL }) => {
  const Environment = IDL.Variant({
    'Local' : IDL.Null,
    'Production' : IDL.Null,
    'Staging' : IDL.Null,
  });
  const UserInit = IDL.Record({
    'handle_canister' : IDL.Principal,
    'principal_origin' : IDL.Text,
    'home_cose' : IDL.Principal,
    'issuer_namespace' : IDL.Text,
    'daily_new_accounts' : IDL.Nat32,
    'payment_canister' : IDL.Principal,
    'environment' : Environment,
    'commerce_canister' : IDL.Principal,
    'governance' : IDL.Principal,
    'max_accounts' : IDL.Nat64,
    'membership_canister' : IDL.Principal,
    'directory_canister' : IDL.Principal,
  });
  return [UserInit];
};
