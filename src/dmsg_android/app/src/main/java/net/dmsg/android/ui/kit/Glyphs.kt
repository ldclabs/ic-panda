package net.dmsg.android.ui.kit

import androidx.annotation.DrawableRes
import net.dmsg.android.R

/**
 * Business icons are the desktop's Remix paths, generated into `res/drawable/glyph_*.xml` by
 * `scripts/ios/sync-icons.py`, which also checks this mapping against the desktop vault types.
 */
@DrawableRes internal fun vaultGlyph(kind: String): Int = when (kind) {
    "login" -> R.drawable.glyph_key
    "api" -> R.drawable.glyph_api
    "key" -> R.drawable.glyph_keypair
    "ssh" -> R.drawable.glyph_ssh
    "wallet" -> R.drawable.glyph_wallet
    "card" -> R.drawable.glyph_card
    "identity" -> R.drawable.glyph_identity
    "server" -> R.drawable.glyph_server
    "database" -> R.drawable.glyph_database
    "license" -> R.drawable.glyph_license
    "note" -> R.drawable.glyph_note
    else -> R.drawable.glyph_file
}
