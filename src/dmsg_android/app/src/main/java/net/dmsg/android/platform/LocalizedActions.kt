package net.dmsg.android.platform

import net.dmsg.android.R

fun approvalTitle(kind: String): String = when (kind) {
    "device_pair" -> str(R.string.text_add_a_device_6dc4f7f); "device_transfer" -> str(R.string.text_replace_a_device_fbe5457)
    "device_revoke" -> str(R.string.text_revoke_a_device_1a089dd); "device_capabilities", "device_control" -> str(R.string.text_change_device_permissions_cb50071)
    "attest", "attestation" -> str(R.string.text_sign_a_document_or_action_6db21d5); "account", "account_control" -> str(R.string.text_change_account_settings_db4ca2f)
    "principal" -> str(R.string.text_change_sign_in_identity_b235b6f); "approval_key" -> str(R.string.text_register_an_approval_key_80f5994)
    "approval_key_retire" -> str(R.string.text_retire_an_approval_key_480fd8b); "hardware_policy" -> str(R.string.text_change_hardware_policy_8d8adbc)
    "secret_share" -> str(R.string.text_deliver_a_secret_snapshot_8075fa5); "secret_package_import" -> str(R.string.text_import_a_secret_snapshot_726c5c9)
    "secret_use" -> str(R.string.text_secret_access_ed712f0); "secret_process" -> str(R.string.text_process_delivery_74a27f9)
    "secret_ssh" -> str(R.string.text_ssh_key_access_7507caf); "secret_mount" -> str(R.string.text_file_mapping_access_3be2c7f)
    "wallet" -> str(R.string.text_team_wallet_action_4615b53); "grant" -> str(R.string.text_change_access_grant_3607d07)
    "delegation" -> str(R.string.text_agent_delegation_968a5a8); "connect" -> str(R.string.text_connected_app_request_baf5966)
    "principal_control" -> str(R.string.security_manage_identity)
    "payment" -> str(R.string.text_payments_01597be)
    "account_deletion" -> str(R.string.text_delete_service_account_1475636)
    "autofill_export" -> str(R.string.text_password_autofill_872af78)
    else -> str(R.string.text_request_8ce9f87) + kind
}
