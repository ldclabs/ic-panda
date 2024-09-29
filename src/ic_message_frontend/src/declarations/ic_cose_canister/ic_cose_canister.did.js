export const idlFactory = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'freezing_threshold' : IDL.Opt(IDL.Nat64),
    'name' : IDL.Opt(IDL.Text),
    'subnet_size' : IDL.Opt(IDL.Nat64),
  });
  const InitArgs = IDL.Record({
    'freezing_threshold' : IDL.Nat64,
    'ecdsa_key_name' : IDL.Text,
    'name' : IDL.Text,
    'schnorr_key_name' : IDL.Text,
    'allowed_apis' : IDL.Vec(IDL.Text),
    'subnet_size' : IDL.Nat64,
    'vetkd_key_name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  const Result = IDL.Variant({ 'Ok' : IDL.Null, 'Err' : IDL.Text });
  const CreateNamespaceInput = IDL.Record({
    'managers' : IDL.Vec(IDL.Principal),
    'desc' : IDL.Opt(IDL.Text),
    'name' : IDL.Text,
    'max_payload_size' : IDL.Opt(IDL.Nat64),
    'auditors' : IDL.Vec(IDL.Principal),
    'users' : IDL.Vec(IDL.Principal),
    'visibility' : IDL.Nat8,
  });
  const NamespaceInfo = IDL.Record({
    'status' : IDL.Int8,
    'updated_at' : IDL.Nat64,
    'managers' : IDL.Vec(IDL.Principal),
    'payload_bytes_total' : IDL.Nat64,
    'desc' : IDL.Text,
    'name' : IDL.Text,
    'max_payload_size' : IDL.Nat64,
    'created_at' : IDL.Nat64,
    'auditors' : IDL.Vec(IDL.Principal),
    'settings_total' : IDL.Nat64,
    'user_settings_total' : IDL.Nat64,
    'users' : IDL.Vec(IDL.Principal),
    'visibility' : IDL.Nat8,
    'gas_balance' : IDL.Nat,
  });
  const Result_1 = IDL.Variant({ 'Ok' : NamespaceInfo, 'Err' : IDL.Text });
  const Result_2 = IDL.Variant({
    'Ok' : IDL.Vec(NamespaceInfo),
    'Err' : IDL.Text,
  });
  const SettingPath = IDL.Record({
    'ns' : IDL.Text,
    'key' : IDL.Vec(IDL.Nat8),
    'subject' : IDL.Opt(IDL.Principal),
    'version' : IDL.Nat32,
    'user_owned' : IDL.Bool,
  });
  const ECDHInput = IDL.Record({
    'public_key' : IDL.Vec(IDL.Nat8),
    'nonce' : IDL.Vec(IDL.Nat8),
  });
  const ECDHOutput = IDL.Record({
    'public_key' : IDL.Vec(IDL.Nat8),
    'payload' : IDL.Vec(IDL.Nat8),
  });
  const Result_3 = IDL.Variant({ 'Ok' : ECDHOutput, 'Err' : IDL.Text });
  const PublicKeyInput = IDL.Record({
    'ns' : IDL.Text,
    'derivation_path' : IDL.Vec(IDL.Vec(IDL.Nat8)),
  });
  const PublicKeyOutput = IDL.Record({
    'public_key' : IDL.Vec(IDL.Nat8),
    'chain_code' : IDL.Vec(IDL.Nat8),
  });
  const Result_4 = IDL.Variant({ 'Ok' : PublicKeyOutput, 'Err' : IDL.Text });
  const SignInput = IDL.Record({
    'ns' : IDL.Text,
    'derivation_path' : IDL.Vec(IDL.Vec(IDL.Nat8)),
    'message' : IDL.Vec(IDL.Nat8),
  });
  const Result_5 = IDL.Variant({ 'Ok' : IDL.Vec(IDL.Nat8), 'Err' : IDL.Text });
  const Result_6 = IDL.Variant({ 'Ok' : IDL.Nat, 'Err' : IDL.Text });
  const UpdateNamespaceInput = IDL.Record({
    'status' : IDL.Opt(IDL.Int8),
    'desc' : IDL.Opt(IDL.Text),
    'name' : IDL.Text,
    'max_payload_size' : IDL.Opt(IDL.Nat64),
    'visibility' : IDL.Opt(IDL.Nat8),
  });
  const SchnorrAlgorithm = IDL.Variant({
    'ed25519' : IDL.Null,
    'bip340secp256k1' : IDL.Null,
  });
  const SignIdentityInput = IDL.Record({
    'ns' : IDL.Text,
    'audience' : IDL.Text,
  });
  const CreateSettingInput = IDL.Record({
    'dek' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'status' : IDL.Opt(IDL.Int8),
    'desc' : IDL.Opt(IDL.Text),
    'tags' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, IDL.Text))),
    'payload' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const CreateSettingOutput = IDL.Record({
    'updated_at' : IDL.Nat64,
    'created_at' : IDL.Nat64,
    'version' : IDL.Nat32,
  });
  const Result_7 = IDL.Variant({
    'Ok' : CreateSettingOutput,
    'Err' : IDL.Text,
  });
  const SettingInfo = IDL.Record({
    'dek' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'key' : IDL.Vec(IDL.Nat8),
    'readers' : IDL.Vec(IDL.Principal),
    'status' : IDL.Int8,
    'updated_at' : IDL.Nat64,
    'subject' : IDL.Principal,
    'desc' : IDL.Text,
    'tags' : IDL.Vec(IDL.Tuple(IDL.Text, IDL.Text)),
    'created_at' : IDL.Nat64,
    'version' : IDL.Nat32,
    'payload' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_8 = IDL.Variant({ 'Ok' : SettingInfo, 'Err' : IDL.Text });
  const SettingArchivedPayload = IDL.Record({
    'dek' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'version' : IDL.Nat32,
    'deprecated' : IDL.Bool,
    'archived_at' : IDL.Nat64,
    'payload' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const Result_9 = IDL.Variant({
    'Ok' : SettingArchivedPayload,
    'Err' : IDL.Text,
  });
  const UpdateSettingInfoInput = IDL.Record({
    'status' : IDL.Opt(IDL.Int8),
    'desc' : IDL.Opt(IDL.Text),
    'tags' : IDL.Opt(IDL.Vec(IDL.Tuple(IDL.Text, IDL.Text))),
  });
  const UpdateSettingPayloadInput = IDL.Record({
    'dek' : IDL.Opt(IDL.Vec(IDL.Nat8)),
    'status' : IDL.Opt(IDL.Int8),
    'deprecate_current' : IDL.Opt(IDL.Bool),
    'payload' : IDL.Opt(IDL.Vec(IDL.Nat8)),
  });
  const StateInfo = IDL.Record({
    'freezing_threshold' : IDL.Nat64,
    'ecdsa_key_name' : IDL.Text,
    'managers' : IDL.Vec(IDL.Principal),
    'name' : IDL.Text,
    'auditors' : IDL.Vec(IDL.Principal),
    'schnorr_key_name' : IDL.Text,
    'allowed_apis' : IDL.Vec(IDL.Text),
    'subnet_size' : IDL.Nat64,
    'namespace_total' : IDL.Nat64,
    'vetkd_key_name' : IDL.Text,
  });
  const Result_10 = IDL.Variant({ 'Ok' : StateInfo, 'Err' : IDL.Text });
  return IDL.Service({
    'admin_add_allowed_apis' : IDL.Func([IDL.Vec(IDL.Text)], [Result], []),
    'admin_add_auditors' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_add_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_create_namespace' : IDL.Func([CreateNamespaceInput], [Result_1], []),
    'admin_list_namespace' : IDL.Func(
        [IDL.Opt(IDL.Text), IDL.Opt(IDL.Nat32)],
        [Result_2],
        ['query'],
      ),
    'admin_remove_allowed_apis' : IDL.Func([IDL.Vec(IDL.Text)], [Result], []),
    'admin_remove_auditors' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'admin_remove_managers' : IDL.Func([IDL.Vec(IDL.Principal)], [Result], []),
    'ecdh_cose_encrypted_key' : IDL.Func(
        [SettingPath, ECDHInput],
        [Result_3],
        [],
      ),
    'ecdsa_public_key' : IDL.Func(
        [IDL.Opt(PublicKeyInput)],
        [Result_4],
        ['query'],
      ),
    'ecdsa_sign' : IDL.Func([SignInput], [Result_5], []),
    'namespace_add_auditors' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'namespace_add_managers' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'namespace_add_users' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'namespace_get_info' : IDL.Func([IDL.Text], [Result_1], ['query']),
    'namespace_remove_auditors' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'namespace_remove_managers' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'namespace_remove_users' : IDL.Func(
        [IDL.Text, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'namespace_top_up' : IDL.Func([IDL.Text, IDL.Nat], [Result_6], []),
    'namespace_update_info' : IDL.Func([UpdateNamespaceInput], [Result], []),
    'schnorr_public_key' : IDL.Func(
        [SchnorrAlgorithm, IDL.Opt(PublicKeyInput)],
        [Result_4],
        ['query'],
      ),
    'schnorr_sign' : IDL.Func([SchnorrAlgorithm, SignInput], [Result_5], []),
    'schnorr_sign_identity' : IDL.Func(
        [SchnorrAlgorithm, SignIdentityInput],
        [Result_5],
        [],
      ),
    'setting_add_readers' : IDL.Func(
        [SettingPath, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'setting_create' : IDL.Func(
        [SettingPath, CreateSettingInput],
        [Result_7],
        [],
      ),
    'setting_get' : IDL.Func([SettingPath], [Result_8], ['query']),
    'setting_get_archived_payload' : IDL.Func(
        [SettingPath],
        [Result_9],
        ['query'],
      ),
    'setting_get_info' : IDL.Func([SettingPath], [Result_8], ['query']),
    'setting_remove_readers' : IDL.Func(
        [SettingPath, IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'setting_update_info' : IDL.Func(
        [SettingPath, UpdateSettingInfoInput],
        [Result_7],
        [],
      ),
    'setting_update_payload' : IDL.Func(
        [SettingPath, UpdateSettingPayloadInput],
        [Result_7],
        [],
      ),
    'state_get_info' : IDL.Func([], [Result_10], ['query']),
    'validate_admin_add_allowed_apis' : IDL.Func(
        [IDL.Vec(IDL.Text)],
        [Result],
        [],
      ),
    'validate_admin_add_auditors' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_add_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_remove_allowed_apis' : IDL.Func(
        [IDL.Vec(IDL.Text)],
        [Result],
        [],
      ),
    'validate_admin_remove_auditors' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'validate_admin_remove_managers' : IDL.Func(
        [IDL.Vec(IDL.Principal)],
        [Result],
        [],
      ),
    'vetkd_encrypted_key' : IDL.Func(
        [SettingPath, IDL.Vec(IDL.Nat8)],
        [Result_5],
        [],
      ),
    'vetkd_public_key' : IDL.Func([SettingPath], [Result_5], []),
  });
};
export const init = ({ IDL }) => {
  const UpgradeArgs = IDL.Record({
    'freezing_threshold' : IDL.Opt(IDL.Nat64),
    'name' : IDL.Opt(IDL.Text),
    'subnet_size' : IDL.Opt(IDL.Nat64),
  });
  const InitArgs = IDL.Record({
    'freezing_threshold' : IDL.Nat64,
    'ecdsa_key_name' : IDL.Text,
    'name' : IDL.Text,
    'schnorr_key_name' : IDL.Text,
    'allowed_apis' : IDL.Vec(IDL.Text),
    'subnet_size' : IDL.Nat64,
    'vetkd_key_name' : IDL.Text,
  });
  const ChainArgs = IDL.Variant({ 'Upgrade' : UpgradeArgs, 'Init' : InitArgs });
  return [IDL.Opt(ChainArgs)];
};
