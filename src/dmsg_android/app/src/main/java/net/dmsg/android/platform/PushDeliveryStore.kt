package net.dmsg.android.platform

import android.content.Context
import android.util.AtomicFile
import net.dmsg.android.core.*
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.json.JSONObject
import java.io.File
import java.time.Instant

/** Independent, non-secret receiver policy. Never opens a workspace or content root. */
class PushDeliveryStore(context: Context) {
    data class Lease(val account: String, val installation: String, val registration: String, val revision: ULong, val expires: Long, val tokenHash: String, val preferences: NoticePreferences)
    private val file = AtomicFile(File(context.noBackupFilesDir, "push-delivery-v1.json"))
    private fun read(): JSONObject = runCatching {
        require(file.baseFile.length() <= MAX_BYTES)
        JSONObject(file.openRead().use { it.readBytes().toString(Charsets.UTF_8) }).also {
            require(it.getInt("version") == 1 && it.getJSONObject("leases").length() <= 16)
        }
    }.getOrElse { JSONObject().put("version", 1).put("sequence", 0L).put("armed", false).put("blocked", true).put("leases", JSONObject()).put("events", JSONObject()).put("pending", JSONObject()).put("suspended", JSONObject()) }
    private fun write(value: JSONObject, policyChange: Boolean = true) {
        if (policyChange) value.put("sequence", value.getLong("sequence") + 1)
        value.put("written", System.currentTimeMillis())
        val bytes = value.toString().toByteArray()
        require(bytes.size <= MAX_BYTES) { "PUSH_STATE_LIMIT" }
        val stream = file.startWrite()
        try { stream.write(bytes); file.finishWrite(stream) }
        catch (error: Throwable) { file.failWrite(stream); throw error }
    }
    fun sequence(): Long = synchronized(lock) { read().getLong("sequence") }
    fun consented(): Boolean = synchronized(lock) { read().optBoolean("armed") }
    fun blocked(): Boolean = synchronized(lock) { read().optBoolean("blocked", true) }
    fun armed(): Boolean = synchronized(lock) { val s = read(); s.optBoolean("armed") && s.optJSONObject("global")?.optBoolean("enabled") == true }
    fun block() = synchronized(lock) { write(read().put("blocked", true)) }
    fun consent(account: String) = synchronized(lock) { val s = read().put("armed", true); s.optJSONObject("suspended")?.remove(account); write(s) }
    fun canRegister(account: String) = synchronized(lock) { read().optJSONObject("suspended")?.optBoolean(account) != true }
    fun lease(account: String): Lease? = synchronized(lock) { read().getJSONObject("leases").optJSONObject(account)?.let { decodeLease(account, it) } }
    fun leases(): List<Lease> = synchronized(lock) {
        val rows = read().getJSONObject("leases")
        rows.keys().asSequence().mapNotNull { id -> rows.optJSONObject(id)?.let { decodeLease(id, it) } }.toList()
    }
    private fun decodeLease(account: String, row: JSONObject): Lease? = runCatching {
        Lease(account, row.getString("installation"), row.getString("registration"), row.getString("revision").toULong(), row.getLong("expires"), row.getString("token_hash"), parseNoticePreferences(Json.parseToJsonElement(row.getString("preferences")))).also {
            require(validAccount(it.account) && validNoticeHash(it.installation) && validNoticeHash(it.registration) && validNoticeHash(it.tokenHash) && it.revision > 0uL && it.preferences.accountId == account)
        }
    }.getOrNull()
    fun forget(account: String) = synchronized(lock) { val s = read(); (s.optJSONObject("suspended") ?: JSONObject().also { s.put("suspended", it) }).put(account, true); s.getJSONObject("leases").remove(account); s.optJSONObject("pending")?.remove(account); s.put("blocked", s.optJSONObject("pending")?.length()?.let { it > 0 } == true); write(s) }
    fun put(value: Lease, preferencesJson: String, expectedSequence: Long): Boolean = synchronized(lock) {
        val s = read(); if (s.getLong("sequence") != expectedSequence || s.optJSONObject("suspended")?.optBoolean(value.account) == true) return@synchronized false
        require(value.preferences.accountId == value.account && value.preferences.account?.enabled == true)
        val rows = s.getJSONObject("leases"); require(rows.has(value.account) || rows.length() < 16)
        rows.put(value.account, JSONObject().put("installation", value.installation).put("registration", value.registration).put("revision", value.revision.toString()).put("expires", value.expires).put("token_hash", value.tokenHash).put("preferences", preferencesJson))
        s.put("global", JSONObject(value.preferences.global.json()))
        // Only a confirmed preference revision can reopen background delivery.
        val pending = s.optJSONObject("pending") ?: JSONObject().also { s.put("pending", it) }
        if (value.preferences.pending || value.preferences.conflict) pending.put(value.account, true) else pending.remove(value.account)
        s.put("blocked", pending.length() > 0)
        write(s); true
    }
    /** Persist the pending local close before any asynchronous preferences save or account cleanup. */
    fun edit(preferences: NoticePreferences) = synchronized(lock) {
        val s = read().put("blocked", true).put("global", JSONObject(preferences.global.json()))
        val pending = s.optJSONObject("pending") ?: JSONObject().also { s.put("pending", it) }
        preferences.accountId?.let { id ->
            if (preferences.account?.enabled == false) { s.getJSONObject("leases").remove(id); pending.remove(id) }
            else pending.put(id, true)
        }
        s.put("blocked", pending.length() > 0)
        write(s)
    }
    fun permitted(value: RemoteNotice, now: Long = System.currentTimeMillis()): Boolean = synchronized(lock) {
        val s = read()
        if (!s.optBoolean("armed") || s.optBoolean("blocked", true) || now < s.optLong("written", now) - 60000 || value.expires <= now) return@synchronized false
        val rows = s.getJSONObject("leases")
        val lease = rows.keys().asSequence().mapNotNull { decodeLease(it, rows.getJSONObject(it)) }.firstOrNull { it.registration == value.registration } ?: return@synchronized false
        if (lease.expires <= now || !value.route.value.startsWith(lease.installation)) return@synchronized false
        val global = runCatching { parseNoticeGlobal(Json.parseToJsonElement(s.getJSONObject("global").toString()).jsonObject) }.getOrNull() ?: return@synchronized false
        global.enabled && global.pausedUntil?.let { it > now.toULong() } != true &&
            (global.quiet?.until(Instant.ofEpochMilli(now)) == null || value.category == "security" && global.securityException) &&
            lease.preferences.account?.let { it.enabled && it.categories[value.category]?.enabled == true } == true
    }
    /** Persist before OS submission, so a restarted receiver cannot repeat the sound. */
    fun seen(value: RemoteNotice, now: Long = System.currentTimeMillis()): Boolean = synchronized(lock) {
        val s = read(); val events = s.getJSONObject("events")
        events.keys().asSequence().toList().filter { events.optLong(it) <= now }.forEach(events::remove)
        val repeated = events.has(value.event)
        if (events.length() >= 1024) return@synchronized true
        events.put(value.event, value.expires); write(s, policyChange = false); repeated
    }
    fun deliver(value: RemoteNotice, post: (Boolean, Boolean) -> Unit): Boolean = synchronized(lock) {
        if (!permitted(value)) return@synchronized false
        val lease = leases().firstOrNull { it.registration == value.registration } ?: return@synchronized false
        val repeated = seen(value)
        post(value.sound && lease.preferences.account?.categories?.get(value.category)?.sound == true, repeated)
        true
    }
    companion object { private const val MAX_BYTES = 2 * 1024 * 1024; private val lock = Any() }
}
