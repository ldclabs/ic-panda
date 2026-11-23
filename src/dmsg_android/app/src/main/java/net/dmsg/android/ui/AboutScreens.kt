package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import net.dmsg.android.BuildConfig

@Composable fun AboutScreen(browser: (String) -> Unit, privacy: () -> Unit) {
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        DetailCard {
            Text("dMsg", style = MaterialTheme.typography.displaySmall)
            Text(str(R.string.text_a_private_workspace_for_the_ai_era_17ce1ec), style = MaterialTheme.typography.titleLarge)
            Text(str(R.string.text_protect_passwords_notes_and_files_with_end_to_end_encryption_shar_85afd73))
            LabelValue(str(R.string.text_version_7e42933), BuildConfig.VERSION_NAME + " (" + BuildConfig.VERSION_CODE + ")")
            if (BuildConfig.IS_STAGING) Text(str(R.string.text_staging_environment_00872b1), color = MaterialTheme.colorScheme.primary)
            Text(str(R.string.text_a_product_of_icpanda_dao_44dd8fa), style = MaterialTheme.typography.bodySmall)
            Text(str(R.string.text_dao_proposals_can_upgrade_on_chain_code_app_and_cloud_service_rel_24439eb), style = MaterialTheme.typography.bodySmall)
        }
        Section {
            ListRow(str(R.string.text_privacy_policy_d216307), onClick = privacy)
            ListRow(str(R.string.text_terms_of_service_01b68b7)) { browser("https://dmsg.net/_/terms") }
            ListRow(str(R.string.text_contact_support_70c3b08)) { browser("https://dmsg.net/_/support") }
            ListRow(str(R.string.text_website_ad80f52)) { browser("https://dmsg.net/") }
            ListRow(str(R.string.text_dao_governance_8b8d8f9)) { browser("https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai") }
        }
    }
}

@Composable fun PrivacyScreen(browser: (String) -> Unit) {
    val context = LocalContext.current
    val locale = LocalConfiguration.current.locales[0]
    val language = when {
        locale.language == "zh" && (locale.script == "Hant" || locale.country in setOf("TW", "HK", "MO")) -> "zh-Hant"
        locale.language == "zh" -> "zh"
        locale.language == "pt" -> "pt-BR"
        locale.language == "tl" -> "fil"
        else -> locale.language
    }
    var document by remember(language) { mutableStateOf<String?>(null) }
    var failed by remember(language) { mutableStateOf(false) }
    LaunchedEffect(language) {
        try {
            document = withContext(Dispatchers.IO) {
                val available = context.assets.list("privacy").orEmpty().toSet()
                val name = ("privacy-" + language + ".md").takeIf { it in available } ?: "privacy-en.md"
                context.assets.open("privacy/" + name).bufferedReader().use { it.readText() }
            }
        } catch (_: Exception) { failed = true }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(pagePadding()), verticalArrangement = Arrangement.spacedBy(18.dp)) {
        Text(str(R.string.text_android_app_59fb6d0), style = MaterialTheme.typography.titleLarge)
        Text(str(R.string.text_android_handles_device_authentication_and_credential_providers_dm_df9a5ad))
        Text(if (net.dmsg.android.BuildConfig.HAS_FCM) str(R.string.text_if_you_enable_background_reminders_this_edition_can_use_google_fi_0942ee1) else str(R.string.text_this_edition_does_not_use_google_services_for_background_reminder_ccc1e1d))
        Text(str(R.string.android_purchase_privacy), style = MaterialTheme.typography.bodySmall)
        HorizontalDivider()
        if (failed) Text(str(R.string.text_the_bundled_policy_could_not_be_opened_d37bb90))
        else if (document == null) CircularProgressIndicator()
        document?.split(Regex("\\n\\s*\\n"))?.filter(String::isNotBlank)?.forEach { paragraph ->
            val plain = paragraph.replace(Regex("\\[([^]]+)]\\([^)]+\\)"), "$1")
            when {
                plain.startsWith("# ") -> Text(plain.removePrefix("# "), style = MaterialTheme.typography.headlineSmall)
                plain.startsWith("## ") -> Text(plain.removePrefix("## "), style = MaterialTheme.typography.titleMedium)
                else -> SelectionContainer { Text(plain, style = MaterialTheme.typography.bodyMedium) }
            }
        }
        TextButton(onClick = { browser("https://dmsg.net/_/privacy") }) { Text(str(R.string.text_read_the_published_policy_3b84e0d)) }
        TextButton(onClick = { browser("https://dmsg.net/_/support") }) { Text(str(R.string.text_contact_support_70c3b08)) }
    }
}

@Composable internal fun PublicInformationDialog(page: String, close: () -> Unit, browser: (String) -> Unit) {
    var current by remember(page) { mutableStateOf(page) }
    fun back() { if (current == "privacy" && page == "about") current = "about" else close() }
    Dialog(onDismissRequest = ::back, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().systemBarsPadding()) {
                Row(Modifier.fillMaxWidth()) {
                    IconButton(onClick = ::back) { Icon(Icons.AutoMirrored.Outlined.ArrowBack, str(R.string.text_back_5db5cac)) }
                    Text(if (current == "privacy") str(R.string.text_privacy_policy_d216307) else str(R.string.text_about_dmsg_630408f), Modifier.padding(12.dp), style = MaterialTheme.typography.titleLarge)
                }
                Box(Modifier.weight(1f)) { if (current == "privacy") PrivacyScreen(browser) else AboutScreen(browser) { current = "privacy" } }
            }
        }
    }
}
