@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package net.dmsg.android.core

import kotlinx.serialization.json.*
import java.time.*

val noticeCategories = listOf("security", "action", "message", "service")
fun tomorrowMorning(timezone: String? = null, now: Instant = Instant.now()): ULong {
    val zone = timezone?.takeIf { it in ZoneId.getAvailableZoneIds() }?.let(ZoneId::of) ?: ZoneId.systemDefault()
    return now.atZone(zone).toLocalDate().plusDays(1).atTime(8, 0).atZone(zone).toInstant().toEpochMilli().toULong()
}
data class NoticeCategory(val enabled: Boolean, val sound: Boolean)
data class NoticeQuietHours(val start: Int, val end: Int, val timezone: String) {
    val valid get() = start in 0..1439 && end in 0..1439 && start != end && timezone in ZoneId.getAvailableZoneIds()
    fun until(now: Instant): ULong? {
        if (!valid) return null
        val zone = ZoneId.of(timezone); val today = now.atZone(zone).toLocalDate()
        for (day in listOf(today.minusDays(1), today)) {
            val startTime = day.atTime(start / 60, start % 60).atZone(zone).withEarlierOffsetAtOverlap().toInstant()
            val endDay = if (end <= start) day.plusDays(1) else day
            val endTime = endDay.atTime(end / 60, end % 60).atZone(zone).withLaterOffsetAtOverlap().toInstant()
            if (now >= startTime && now < endTime) return endTime.toEpochMilli().toULong()
        }
        return null
    }
}
data class NoticeGlobal(val revision: ULong, val enabled: Boolean, val pausedUntil: ULong?, val quiet: NoticeQuietHours?, val securityException: Boolean, val badge: String) {
    val valid get() = badge in setOf("all", "actions", "off") && quiet?.valid != false
    fun json() = buildJsonObject {
        put("revision", JsonPrimitive(revision)); put("enabled", enabled); put("paused_until", pausedUntil?.let(::JsonPrimitive) ?: JsonNull)
        put("quiet_hours", quiet?.let { buildJsonObject { put("start", it.start); put("end", it.end); put("timezone", it.timezone) } } ?: JsonNull)
        put("security_exception", securityException); put("badge", badge)
    }.toString()
}
data class NoticeAccount(val revision: ULong, val enabled: Boolean, val categories: Map<String, NoticeCategory>, val mergeDevices: Boolean, val agentMail: Map<String, Boolean>, val remoteRevision: ULong) {
    fun json() = buildJsonObject {
        put("revision", JsonPrimitive(revision)); put("enabled", enabled); put("merge_devices", mergeDevices); put("remote_revision", JsonPrimitive(remoteRevision))
        putJsonObject("categories") { categories.forEach { (key, value) -> putJsonObject(key) { put("enabled", value.enabled); put("sound", value.sound) } } }
        putJsonObject("agent_mail") { agentMail.forEach { (key, value) -> put(key, value) } }
    }.toString()
}
data class NoticePreferences(val global: NoticeGlobal, val account: NoticeAccount?, val accountId: String?, val remoteSupported: Boolean, val globalPending: Boolean, val globalConflict: Boolean, val accountConflict: Boolean, val registrationRequired: Boolean) {
    val pending get() = globalPending || account?.let { it.revision > it.remoteRevision } == true
    val conflict get() = globalConflict || accountConflict
}
fun parseNoticeGlobal(row: JsonObject): NoticeGlobal {
    val quiet = row["quiet_hours"]?.takeIf { it != JsonNull }?.objectValue()?.let {
        val start = it.requiredUnsigned("start"); val end = it.requiredUnsigned("end")
        check(start < 1440uL && end < 1440uL) { "INTEGRITY_FAILED" }
        NoticeQuietHours(start.toInt(), end.toInt(), it.required("timezone"))
    }
    return NoticeGlobal(row.requiredUnsigned("revision"), row.requiredFlag("enabled"), row.unsigned("paused_until"), quiet, row.requiredFlag("security_exception"), row.required("badge")).also { check(it.valid) { "INTEGRITY_FAILED" } }
}
fun parseNoticeAccount(row: JsonObject): NoticeAccount {
    val categories = row["categories"]!!.objectValue().mapValues { (_, value) -> value.objectValue().let { NoticeCategory(it.requiredFlag("enabled"), it.requiredFlag("sound")) } }
    check(categories.keys == noticeCategories.toSet()) { "INTEGRITY_FAILED" }
    val mail = row["agent_mail"]!!.objectValue()
    return NoticeAccount(row.requiredUnsigned("revision"), row.requiredFlag("enabled"), categories, row.requiredFlag("merge_devices"), mail.keys.associateWith { mail.requiredFlag(it) }, row.requiredUnsigned("remote_revision"))
}
fun parseNoticePreferences(value: JsonElement): NoticePreferences = value.objectValue().let {
    NoticePreferences(parseNoticeGlobal(it["global"]!!.objectValue()), it["account"]?.takeIf { it != JsonNull }?.objectValue()?.let(::parseNoticeAccount), it.text("account_id"), it.requiredFlag("remote_supported"), it.requiredFlag("global_pending_sync"), it.requiredFlag("global_conflict"), it.requiredFlag("account_conflict"), it.requiredFlag("registration_required"))
}
data class NoticeCounts(val actions: ULong, val unread: ULong, val unverified: Boolean) {
    val total get() = if (ULong.MAX_VALUE - actions < unread) ULong.MAX_VALUE else actions + unread
}
data class NoticeAccountCounts(val account: String, val enabled: Boolean, val counts: NoticeCounts)
data class NoticeStatus(val counts: NoticeCounts, val accounts: List<NoticeAccountCounts>, val badge: ULong)
private fun JsonObject.noticeCounts() = NoticeCounts(requiredUnsigned("actionable"), requiredUnsigned("unread"), requiredFlag("unverified"))
fun parseNoticeStatus(value: JsonElement) = value.objectValue().let { row ->
    NoticeStatus(row["counts"]!!.objectValue().noticeCounts(), row.rows("accounts").map { NoticeAccountCounts(it.required("account"), it.requiredFlag("enabled"), it["counts"]!!.objectValue().noticeCounts()) }, row.requiredUnsigned("badge"))
}
fun canonicalNoticeVersion(text: String): ULong = text.toULongOrNull()?.takeIf { it.toString() == text } ?: error("INTEGRITY_FAILED")
private fun noticeHash(text: String?) = text != null && text.length == 64 && text.all { it in '0'..'9' || it in 'a'..'f' }
data class NoticeTarget(val kind: String, val id: String?, val wallet: String? = null) {
    val category get() = when (kind) { "recovery" -> "security"; "approval", "task", "wallet" -> "action"; "plan", "service" -> "service"; "summary" -> id!!; else -> "message" }
    fun json() = buildJsonObject {
        put("kind", kind)
        put("id", if (kind == "wallet") buildJsonObject { put("wallet", wallet); put("proposal", id) } else id?.let(::JsonPrimitive) ?: JsonNull)
    }.toString()
}
fun parseNoticeTarget(value: JsonElement): NoticeTarget {
    val row = value.objectValue(); val kind = row.required("kind")
    if (kind == "wallet") {
        val wallet = row["id"]!!.objectValue(); canonicalNoticeVersion(wallet.required("proposal"))
        return NoticeTarget(kind, wallet.required("proposal"), wallet.required("wallet"))
    }
    val id = row.text("id")
    check(when (kind) {
        "channel", "approval", "recovery", "plan" -> noticeHash(id)
        "inbox" -> id == null || noticeHash(id)
        "summary" -> id in noticeCategories
        "task" -> id != null && id.length <= 256 && (id.startsWith("conflicts:") || id.startsWith("sync_failed:"))
        "service" -> id != null && id.length <= 256 && (id.startsWith("network:") || id.startsWith("registration:"))
        "agent_mail" -> id != null && id.startsWith("did:agent:") && id.length <= 256
        else -> false
    }) { "INTEGRITY_FAILED" }
    return NoticeTarget(kind, id)
}
data class ReminderItem(val id: String, val account: String, val target: NoticeTarget, val version: ULong, val readVersion: ULong, val observed: ULong, val open: Boolean, val actionable: Boolean, val unread: ULong?, val unverified: Boolean, val updatedAt: ULong, val deadline: ULong?, val snoozedUntil: ULong?) {
    val isUnread get() = readVersion < version
}
fun parseReminder(value: JsonElement) = value.objectValue().let {
    val id = it.required("id"); val version = canonicalNoticeVersion(it.required("version"))
    check(noticeHash(id) && validAccount(it.required("account")) && version > 0u) { "INTEGRITY_FAILED" }
    ReminderItem(id, it.required("account"), parseNoticeTarget(it["target"]!!), version, canonicalNoticeVersion(it.required("read_version")), canonicalNoticeVersion(it.required("observed")), it.requiredFlag("open"), it.requiredFlag("actionable"), it.unsigned("unread"), it.requiredFlag("unverified"), it.requiredUnsigned("updated_at"), it.unsigned("deadline"), it.unsigned("snoozed_until"))
}
data class ReminderPage(val items: List<ReminderItem>, val next: UInt?)
fun parseReminderPage(value: JsonElement): ReminderPage {
    val row = value.objectValue(); val next = row.unsigned("next")
    check(next == null || next <= UInt.MAX_VALUE.toULong()) { "INTEGRITY_FAILED" }
    return ReminderPage(row.rows("items").map(::parseReminder), next?.toUInt())
}
