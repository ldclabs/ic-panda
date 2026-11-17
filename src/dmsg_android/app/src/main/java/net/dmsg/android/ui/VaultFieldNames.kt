package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str

/** Only unnamed built-in fields are translated. User-authored labels stay exact. */
internal fun vaultFieldTitle(id: String, kind: String, label: String): String = label.ifBlank {
    if (kind == "otp") str(R.string.text_one_time_code_c7b1bc6) else vaultFieldName(id)
}

internal fun vaultFieldName(id: String): String = when (id) {
    "address" -> str(R.string.text_address_d082ce5)
    "birthday" -> str(R.string.vault_field_birthday)
    "cardholder" -> str(R.string.vault_field_cardholder)
    "credential" -> str(R.string.vault_field_credential)
    "cvv" -> str(R.string.vault_field_cvv)
    "database" -> str(R.string.text_database_7bc9ec0)
    "email" -> str(R.string.text_email_6d5db1e)
    "engine" -> str(R.string.vault_field_engine)
    "expires" -> str(R.string.text_expires_12414d7)
    "expiry" -> str(R.string.vault_field_expiry)
    "fingerprint" -> str(R.string.vault_field_fingerprint)
    "host" -> str(R.string.vault_field_host)
    "id_number" -> str(R.string.vault_field_id_number)
    "key" -> str(R.string.text_key_4b6b6ae)
    "license_key" -> str(R.string.vault_field_license_key)
    "licensed_to" -> str(R.string.vault_field_licensed_to)
    "name" -> str(R.string.text_name_dd4dc4c)
    "network" -> str(R.string.vault_field_network)
    "number" -> str(R.string.vault_field_number)
    "password" -> str(R.string.text_password_347d7fc)
    "phone" -> str(R.string.text_phone_9460aa6)
    "phrase" -> str(R.string.vault_field_phrase)
    "pin" -> str(R.string.vault_field_pin)
    "port" -> str(R.string.vault_field_port)
    "private_key" -> str(R.string.vault_field_private_key)
    "public_key" -> str(R.string.vault_field_public_key)
    "purchased" -> str(R.string.vault_field_purchased)
    "url" -> str(R.string.text_website_ad80f52)
    "username" -> str(R.string.vault_field_username)
    else -> if (id.startsWith("field_")) str(R.string.text_field_name_fb3c7fc) else id.replace('_', ' ')
}
