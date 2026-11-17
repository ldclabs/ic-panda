package net.dmsg.android.core

import kotlinx.serialization.json.*

data class NoticeCoverage(val id: String, val version: ULong, val stage: UInt)
data class NoticeDelivery(val systemId: String, val deliveryId: String, val account: String, val category: String, val target: NoticeTarget, val covered: List<NoticeCoverage>, val count: ULong?, val sound: Boolean, val inApp: Boolean, val update: Boolean) {
    val generic get() = category != "security" && count == null
}
data class NoticeBatch(val plans: List<NoticeDelivery>, val remove: List<String>, val nextDue: ULong?)
internal fun validNoticeHash(value: String) = value.length == 64 && value.all { it in 'a'..'f' || it in '0'..'9' }
fun parseNoticeDelivery(value: JsonElement): NoticeDelivery = value.objectValue().let { row ->
    val category = row.required("category").also { check(it in noticeCategories) { "INTEGRITY_FAILED" } }
    val target = parseNoticeTarget(row["target"]!!)
    check(target.category == category) { "INTEGRITY_FAILED" }
    val covered = row.rows("covered").map { NoticeCoverage(it.required("id").also { id -> check(validNoticeHash(id)) }, canonicalNoticeVersion(it.required("version")), it.requiredUInt("stage").also { stage -> check(stage <= 255u) }) }
    check(covered.size in 1..1000 && covered.map { it.id }.distinct().size == covered.size) { "INTEGRITY_FAILED" }
    NoticeDelivery(row.required("system_id").also { check(validNoticeHash(it)) }, row.required("delivery_id").also { check(validNoticeHash(it)) }, row.required("account").also { check(validAccount(it)) }, category, target, covered,
        row.unsigned("count"), row.requiredFlag("sound"), row.requiredFlag("in_app"), row.requiredFlag("update"))
}
fun parseNoticeBatch(value: JsonElement): NoticeBatch = value.objectValue().let { row ->
    val plans = row.rows("plans").map(::parseNoticeDelivery)
    val remove = row.strings("remove")
    check(plans.size <= 1000 && remove.size <= 1000 && remove.all(::validNoticeHash)) { "INTEGRITY_FAILED" }
    NoticeBatch(plans, remove, row.unsigned("next_due"))
}
