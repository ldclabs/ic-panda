package net.dmsg.android.platform

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import kotlinx.serialization.json.*
import net.dmsg.android.core.*
import java.net.IDN
import java.net.URI
import javax.net.ssl.HttpsURLConnection

/** A WebView's domain is a claim. Only a trusted browser or a
 * site's credential-sharing statement can authorize its installed signer. */
object AutofillTrust {
    fun displayOrigin(value: String): String = runCatching {
        val text = value.trim()
        val uri = URI(if (text.contains("://")) text else "https://$text")
        val scheme = uri.scheme.lowercase()
        require(scheme == "http" || scheme == "https")
        val authority = requireNotNull(uri.rawAuthority).substringAfterLast('@')
        "$scheme://$authority"
    }.getOrDefault("")

    fun host(value: String): String? = runCatching {
        val uri = URI(if (value.contains("://")) value else "https://$value")
        require(uri.scheme == "https" && uri.rawUserInfo == null && uri.port in setOf(-1, 443))
        val host = IDN.toASCII(requireNotNull(uri.host), IDN.USE_STD3_ASCII_RULES).lowercase()
        require(host.length in 3..253 && '.' in host && !host.endsWith('.') && !host.matches(Regex("[0-9.]+")) && ':' !in host)
        host
    }.getOrNull()
    fun fingerprints(context: Context, packageName: String): Set<String> {
        val info = context.packageManager.getPackageInfo(packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        return checkNotNull(info.signingInfo).apkContentsSigners.map { SystemExportStore.digest(it.toByteArray()).uppercase() }.toSet().also { check(it.isNotEmpty()) { "AUTOFILL_UNVERIFIED" } }
    }
    fun statement(json: String, packageName: String, fingerprints: Set<String>): Boolean = runCatching {
        check(json.toByteArray().size <= 65536 && fingerprints.isNotEmpty())
        Json.parseToJsonElement(json).arrayRows().takeIf { it.size <= 100 }?.any { row ->
            val target = row["target"]?.objectValue() ?: return@any false
            row.strings("relation").contains("delegate_permission/common.get_login_creds") && target.text("namespace") == "android_app" && target.text("package_name") == packageName &&
                target.strings("sha256_cert_fingerprints").map { it.replace(":", "").uppercase() }.filter { it.matches(Regex("[A-F0-9]{64}")) }.toSet().containsAll(fingerprints)
        } == true
    }.getOrDefault(false)
    fun associated(origin: String, packageName: String, fingerprints: Set<String>): Boolean {
        val host = host(origin) ?: return false
        val connection = URI("https://$host/.well-known/assetlinks.json").toURL().openConnection() as HttpsURLConnection
        try {
            connection.connectTimeout = 5000; connection.readTimeout = 5000; connection.instanceFollowRedirects = false
            if (connection.responseCode != 200) return false
            val bytes = connection.inputStream.use { it.readChunk(65537) }
            return bytes.size <= 65536 && statement(bytes.toString(Charsets.UTF_8), packageName, fingerprints)
        } finally { connection.disconnect() }
    }
    fun isBrowser(context: Context, packageName: String): Boolean {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://autofill.invalid")).addCategory(Intent.CATEGORY_BROWSABLE).setPackage(packageName)
        return context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_ALL).isNotEmpty()
    }

    fun browserCandidates(context: Context): List<BrowserTrust> {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://example.com")).addCategory(Intent.CATEGORY_BROWSABLE)
        return context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_ALL).mapNotNull { row ->
            runCatching { val pkg = row.activityInfo.packageName; BrowserTrust(pkg, row.loadLabel(context.packageManager).toString().take(100), fingerprints(context, pkg)) }.getOrNull()
        }.distinctBy { it.packageName }.sortedBy { it.label }
    }
}
data class BrowserTrust(val packageName: String, val label: String, val fingerprints: Set<String>)
class TrustedBrowsers(context: Context) {
    private val preferences = context.getSharedPreferences("system-autofill-browsers", Context.MODE_PRIVATE)
    fun trusted(packageName: String, fingerprints: Set<String>) =
        fingerprints.isNotEmpty() && preferences.getStringSet(packageName, null) == fingerprints

    fun set(browser: BrowserTrust, trusted: Boolean) {
        check(browser.packageName.matches(Regex("[A-Za-z0-9_.]{1,255}")) && browser.fingerprints.isNotEmpty())
        check(preferences.edit().apply {
            if (trusted) putStringSet(browser.packageName, browser.fingerprints) else remove(browser.packageName)
        }.commit())
    }
}
