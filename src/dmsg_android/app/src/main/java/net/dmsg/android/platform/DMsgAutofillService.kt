package net.dmsg.android.platform

import net.dmsg.android.R
import android.app.PendingIntent
import android.app.assist.AssistStructure
import android.content.Context
import android.content.Intent
import android.os.CancellationSignal
import android.service.autofill.*
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import android.widget.RemoteViews
import java.util.UUID

data class AutofillTarget(val packageName: String, val fingerprints: Set<String>, val password: AutofillId, val username: AutofillId?, val host: String?, val web: Boolean, val secureWeb: Boolean, val displayName: String) {
    val ids: Array<AutofillId> get() = listOfNotNull(password, username).distinct().toTypedArray()
}
object AutofillRequests {
    // Android can reuse the authenticated dataset's placeholder for this form.
    private val requests = linkedMapOf<String, AutofillTarget>()
    @Synchronized fun put(target: AutofillTarget): String {
        while (requests.size >= 8) requests.remove(requests.keys.first())
        return UUID.randomUUID().toString().also { requests[it] = target }
    }
    @Synchronized fun get(id: String): AutofillTarget? = requests[id]
    @Synchronized fun remove(id: String) { requests.remove(id) }
    @Synchronized fun clear() { requests.clear() }
}

/** The system binds this service. Returned suggestions contain no login data. */
class DMsgAutofillService : AutofillService() {
    override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
        try {
            if (cancellationSignal.isCanceled) { callback.onSuccess(null); return }
            val structure = request.fillContexts.lastOrNull()?.structure
            val target = structure?.let { target(this, it) }
            if (target == null) { callback.onSuccess(null); return }
            val id = AutofillRequests.put(target)
            cancellationSignal.setOnCancelListener { AutofillRequests.remove(id) }
            val intent = Intent(this, ExportAccessActivity::class.java).putExtra("request", id)
            val pending = PendingIntent.getActivity(this, id.hashCode(), intent, PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            val presentation = RemoteViews(packageName, android.R.layout.simple_list_item_1).apply {
                setTextViewText(android.R.id.text1, str(R.string.text_unlock_dmsg_passwords_7904a03))
            }
            val picker = Dataset.Builder(presentation).setAuthentication(pending.intentSender)
            target.ids.forEach { picker.setValue(it, null) }
            if (!cancellationSignal.isCanceled) callback.onSuccess(FillResponse.Builder().addDataset(picker.build()).build())
        } catch (_: Exception) { if (!cancellationSignal.isCanceled) callback.onSuccess(null) }
    }
    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) { callback.onFailure(str(R.string.text_save_login_items_in_dmsg_first_2639bb4)) }
    companion object {
        internal fun target(context: Context, structure: AssistStructure): AutofillTarget? {
            val pkg = structure.activityComponent?.packageName ?: return null
            if (pkg == context.packageName) return null
            data class Field(val id: AutofillId, val window: Int, val parent: AssistStructure.ViewNode?, val password: Boolean, val username: Boolean, val focused: Boolean, val domain: String?, val web: Boolean, val scheme: String?)
            val fields = mutableListOf<Field>(); var visited = 0
            fun walk(node: AssistStructure.ViewNode, window: Int, parent: AssistStructure.ViewNode?, inheritedDomain: String?, inheritedScheme: String?, inWeb: Boolean, depth: Int) {
                check(++visited <= 4096 && depth < 64)
                val domain = node.webDomain?.also { check(it.length <= 512) } ?: inheritedDomain
                val scheme = node.webScheme ?: inheritedScheme
                val web = inWeb || domain != null || node.htmlInfo != null || node.className == "android.webkit.WebView"
                val hints = node.autofillHints?.toSet().orEmpty()
                val variation = node.inputType and InputType.TYPE_MASK_VARIATION
                val isText = node.inputType and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_TEXT
                val password = View.AUTOFILL_HINT_PASSWORD in hints || isText && variation in setOf(InputType.TYPE_TEXT_VARIATION_PASSWORD, InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD)
                val username = View.AUTOFILL_HINT_USERNAME in hints || View.AUTOFILL_HINT_EMAIL_ADDRESS in hints || isText && variation in setOf(InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS, InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS)
                if (node.autofillType == View.AUTOFILL_TYPE_TEXT && node.autofillId != null && (password || username)) fields += Field(node.autofillId!!, window, parent, password, username, node.isFocused, domain, web, scheme)
                for (i in 0 until node.childCount) walk(node.getChildAt(i), window, node, domain, scheme, web, depth + 1)
            }
            for (i in 0 until structure.windowNodeCount) walk(structure.getWindowNodeAt(i).rootViewNode, i, null, null, null, false, 0)
            val passwords = fields.filter { it.password }
            val password = passwords.singleOrNull() ?: passwords.singleOrNull { it.focused } ?: return null
            val host = password.domain?.let(AutofillTrust::host)
            val names = fields.filter { it.username && !it.password && it.window == password.window && it.domain == password.domain && it.web == password.web }
            val username = names.singleOrNull { it.parent === password.parent } ?: names.singleOrNull()?.takeIf { !password.web && passwords.size == 1 }
            val label = runCatching { context.packageManager.getApplicationLabel(context.packageManager.getApplicationInfo(pkg, 0)).toString() }.getOrDefault(pkg)
            return AutofillTarget(pkg, AutofillTrust.fingerprints(context, pkg), password.id, username?.id, host, password.web, password.scheme.equals("https", ignoreCase = true), if (label == pkg) pkg else "$label ($pkg)")
        }
    }
}
