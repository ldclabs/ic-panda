package net.dmsg.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.serialization.json.*
import net.dmsg.android.BuildConfig
import net.dmsg.core.*

data class FeedbackReport(val id: String, val revision: ULong, val recipient: String, val text: String, val context: String,
    val screenshots: List<FeedbackImage>, val state: String, val error: String?, val editable: Boolean)
data class FeedbackImage(val file: String, val version: String, val name: String, val size: ULong, val mime: String, val url: String?, val expiresAt: ULong?)
fun parseFeedback(value: JsonElement): FeedbackReport? {
    if (value == JsonNull) return null
    val row = value.objectValue()
    return FeedbackReport(row.required("id"), row.requiredUnsigned("revision"), row.required("recipient"), row.required("text"), row.required("context"),
        row.rows("screenshots").map { FeedbackImage(it.required("file"), it.required("version"), it.required("name"), it.requiredUnsigned("size"), it.required("mime"), it.text("url"), it.unsigned("expires_at")) },
        row.required("state"), row.text("error"), row.requiredFlag("editable"))
}
class FeedbackCoordinator(private val client: ClientController) {
    val report = ReadStore<FeedbackReport?> { it == null }
    private val mutable = MutableStateFlow<FeedbackDraftInput?>(null)
    val draft = mutable.asStateFlow()
    private val turn = Mutex()
    private var dirty = false
    fun load() {
        val request = report.begin()
        client.perform({ parseFeedback(it.feedback().decode()) }, key = "feedback:read", replace = true,
            onError = { report.fail(request, it) }) { value ->
            if (!report.accept(request, value)) return@perform
            if (!dirty) mutable.value = if (value == null) FeedbackDraftInput(requestId(), 0u, "", "dMsg ${BuildConfig.VERSION_NAME} · Android ${android.os.Build.VERSION.RELEASE}", emptyList())
            else FeedbackDraftInput(value.id, value.revision, value.text, value.context, value.screenshots.map { it.file })
        }
    }
    fun update(value: FeedbackDraftInput) {
        client.activity()
        if (client.isWorking("feedback:send") || client.isWorking("feedback:discard") || report.value()?.state?.let { it != "draft" } == true) return
        mutable.value = value; dirty = true
        client.perform({ core ->
            val current = parseFeedback(core.feedback().decode())
            check(current == null || current.id == value.id && current.state == "draft") { "VERSION_CONFLICT" }
            parseFeedback(core.saveFeedback(value.copy(revision = current?.revision ?: 0u)).decode())!!
        }, key = "feedback:save", replace = true, serial = turn) { result ->
            if (mutable.value == value) { mutable.value = value.copy(revision = result.revision); dirty = false }
            val request = report.begin(); report.accept(request, result)
        }
    }
    fun send() {
        val value = mutable.value ?: return
        if (client.isWorking("feedback:discard")) return
        client.perform({ core ->
            val current = parseFeedback(core.feedback().decode())
            check(current == null || current.id == value.id) { "VERSION_CONFLICT" }
            val saved = if (current?.state == "draft" || current == null) parseFeedback(core.saveFeedback(value.copy(revision = current?.revision ?: 0u)).decode())!! else current
            client.operation { core.sendFeedback(saved.id, saved.revision, it) }
        }, key = "feedback:send", serial = turn, onError = { load() }) { dirty = false; load() }
    }
    fun edit() { val id = report.value()?.id ?: return; client.perform({ it.editFeedback(id) }, key = "feedback:edit", serial = turn) { load() } }
    fun discard() {
        val value = report.value() ?: return
        if (client.isWorking("feedback:send")) return
        client.perform({ it.clearFeedback(value.id, value.revision) }, key = "feedback:discard", serial = turn, onError = { load() }) {
            dirty = false; mutable.value = null; load()
        }
    }
    internal fun restore(input: FeedbackDraftInput) {
        if (!dirty) { mutable.value = input; dirty = true }
    }
    fun pending() = mutable.value?.takeIf { dirty && !client.isWorking("feedback:discard") }
    fun acknowledge(value: FeedbackDraftInput?) { if (value != null && value == mutable.value) dirty = false }
    fun clear(retainDrafts: Boolean = false) { report.reset(); if (!retainDrafts) { mutable.value = null; dirty = false } }
}
