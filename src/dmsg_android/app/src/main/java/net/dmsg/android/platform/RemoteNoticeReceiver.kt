package net.dmsg.android.platform

import android.content.Context
import net.dmsg.android.DMsgApplication
import net.dmsg.android.core.RemoteNotice

/** Shared with the OS receiver tests; this component owns no workspace handle. */
class RemoteNoticeReceiver(context: Context) {
    enum class Result { INVALID, SUPPRESSED, FOREGROUND, SUBMITTED, FAILED }
    private val app = context.applicationContext as DMsgApplication
    fun receive(sender: String?, data: Map<String, String>, configuration: GoogleConfiguration): Result = runCatching {
        if (sender != configuration.projectNumber.toString()) return@runCatching Result.INVALID
        val plan = RemoteNotice.parse(data, System.currentTimeMillis()) ?: return@runCatching Result.INVALID
        val policy = PushDeliveryStore(app)
        if (!policy.permitted(plan)) return@runCatching Result.SUPPRESSED
        if (app.receiveNoticeHint()) { policy.seen(plan); return@runCatching Result.FOREGROUND }
        var result = Result.SUPPRESSED
        policy.deliver(plan) { sound, repeated ->
            result = when (SystemNotices(app).post(SystemNotices.Presentation(plan.systemId, plan.route, plan.category, sound, repeated, plan.generic, registration = plan.registration))) {
                SystemNotices.Result.SUBMITTED -> Result.SUBMITTED
                SystemNotices.Result.SUPPRESSED -> Result.SUPPRESSED
                SystemNotices.Result.FAILED -> Result.FAILED
            }
        }
        result
    }.getOrDefault(Result.FAILED)
}
