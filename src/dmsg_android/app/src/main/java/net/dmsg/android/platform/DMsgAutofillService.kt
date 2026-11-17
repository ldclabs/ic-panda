package net.dmsg.android.platform

import net.dmsg.android.R
import android.app.PendingIntent
import android.app.assist.AssistStructure
import android.content.Context
import android.content.Intent
import android.os.CancellationSignal
import android.os.SystemClock
import android.service.autofill.*
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import android.widget.RemoteViews
import java.util.UUID

data class AutofillTarget(val packageName: String, val fingerprints: Set<String>, val password: AutofillId, val username: AutofillId?, val host: String?) {
    val ids: Array<AutofillId> get() = listOfNotNull(password, username).distinct().toTypedArray()
}
object AutofillRequests {
    private data class Request(val target: AutofillTarget, val expires: Long)
    private val requests = linkedMapOf<String, Request>()
    @Synchronized fun put(target: AutofillTarget): String {
        requests.entries.removeAll { it.value.expires <= SystemClock.elapsedRealtime() }
        while (requests.size >= 8) requests.remove(requests.keys.first())
        return UUID.randomUUID().toString().also { requests[it] = Request(target, SystemClock.elapsedRealtime() + 120_000) }
    }
    @Synchronized fun get(id: String): AutofillTarget? = requests[id]?.takeIf { it.expires > SystemClock.elapsedRealtime() }?.target
    @Synchronized fun remove(id: String) { requests.remove(id) }
    @Synchronized fun clear() { requests.clear() }
}

/** The system binds this service. Returned suggestions contain no login data. */
class DMsgAutofillService : AutofillService() {
    override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
        try {
            val store = systemExports(this, ExportPurpose.credentials)
            if (cancellationSignal.isCanceled || store.header() == null) { callback.onSuccess(null); return }
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
            if (!cancellationSignal.isCanceled) callback.onSuccess(FillResponse.Builder().setAuthentication(target.ids, pending.intentSender, presentation).build())
        } catch (_: Exception) { if (!cancellationSignal.isCanceled) callback.onSuccess(null) }
    }
    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) { callback.onFailure(str(R.string.text_save_login_items_in_dmsg_first_2639bb4)) }
    companion object {
        internal fun target(context: Context, structure: AssistStructure): AutofillTarget? {
            val pkg = structure.activityComponent?.packageName ?: return null
            if (pkg == context.packageName) return null
            data class Field(val id: AutofillId, val parent: AssistStructure.ViewNode?, val password: Boolean, val username: Boolean, val focused: Boolean, val domain: String?)
            val fields = mutableListOf<Field>(); var visited = 0
            fun walk(node: AssistStructure.ViewNode, parent: AssistStructure.ViewNode?, web: String?, depth: Int) {
                check(++visited <= 4096 && depth < 64)
                val domain = node.webDomain?.also { check(it.length <= 512) } ?: web
                val hints = node.autofillHints?.toSet().orEmpty()
                val variation = node.inputType and InputType.TYPE_MASK_VARIATION
                val isText = node.inputType and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_TEXT
                val password = View.AUTOFILL_HINT_PASSWORD in hints || isText && variation in setOf(InputType.TYPE_TEXT_VARIATION_PASSWORD, InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD)
                val username = View.AUTOFILL_HINT_USERNAME in hints || View.AUTOFILL_HINT_EMAIL_ADDRESS in hints || isText && variation in setOf(InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS, InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS)
                if (node.autofillType == View.AUTOFILL_TYPE_TEXT && node.autofillId != null && (password || username)) fields += Field(node.autofillId!!, parent, password, username, node.isFocused, domain)
                for (i in 0 until node.childCount) walk(node.getChildAt(i), node, domain, depth + 1)
            }
            for (i in 0 until structure.windowNodeCount) walk(structure.getWindowNodeAt(i).rootViewNode, null, null, 0)
            val passwords = fields.filter { it.password }
            val password = passwords.singleOrNull() ?: passwords.singleOrNull { it.focused } ?: return null
            val host = password.domain?.let { AutofillTrust.host(it) ?: return null }
            val username = fields.singleOrNull { it.username && !it.password && it.parent === password.parent && it.domain == password.domain }
            return AutofillTarget(pkg, AutofillTrust.fingerprints(context, pkg), password.id, username?.id, host)
        }
    }
}
