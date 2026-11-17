package net.dmsg.android.core

import android.graphics.Bitmap
import android.net.Uri
import android.util.Base64
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.*
import net.dmsg.android.platform.BoundedImages
import net.dmsg.android.platform.PrivateFiles
import net.dmsg.android.platform.ProfileImages
import java.io.File
import java.net.URI

data class ProfileLink(val name: String, val url: String, val relation: String = "related")
data class PublicProfile(val name: String?, val about: String?, val avatar: String?, val links: List<ProfileLink>, val updatedAt: Long)
data class ProfileDraft(val name: String = "", val about: String = "", val avatar: String? = null, val links: List<ProfileLink> = emptyList(), val avatarChanged: Boolean = false) {
    val valid get() = name.codePointCount(0, name.length) <= 80 && about.codePointCount(0, about.length) <= 2000 && links.size <= 100 && links.all { link ->
        link.name.isNotBlank() && link.url.toByteArray(Charsets.UTF_8).size <= 2048 && runCatching { URI(link.url).let { it.scheme?.lowercase() == "https" && it.host != null && it.rawUserInfo == null } }.getOrDefault(false)
    }
    fun linksJson() = buildJsonArray { links.forEach { link -> add(buildJsonObject { put("name", link.name.trim()); put("url", link.url.trim()); put("rel", link.relation) }) } }.toString()
}
data class ProfilePublication(val published: Boolean, val pending: Boolean, val updatedAt: Long?)
fun parsePublicProfile(value: JsonElement): PublicProfile {
    val row = value.objectValue()
    val at = row.requiredUnsigned("updated_at"); check(at <= Long.MAX_VALUE.toULong()) { "INTEGRITY_FAILED" }
    return PublicProfile(row.text("name"), row.text("description"), row.text("avatar_url"), row.rows("links").map { ProfileLink(it.required("name"), it.required("url"), it.text("rel") ?: "related") }, at.toLong())
}
fun parseProfileSnapshot(value: JsonElement): PublicProfile? {
    val row = value.objectValue(); val state = row.required("state")
    if (state == "missing") { check(row["data"] == JsonNull); return null }
    check(state == "present") { "INTEGRITY_FAILED" }
    row.requiredUnsigned("observed_at_ms"); row.requiredFlag("stale")
    return parsePublicProfile(row["data"]!!)
}
fun parseProfilePublication(value: JsonElement): ProfilePublication {
    val row = value.objectValue(); val at = row.unsigned("updated_at"); check(at == null || at <= Long.MAX_VALUE.toULong()) { "INTEGRITY_FAILED" }
    return ProfilePublication(row.requiredFlag("published"), row.flag("pending"), at?.toLong())
}
private fun PublicProfile.draft() = ProfileDraft(name.orEmpty(), about.orEmpty(), avatar, links)

/** The form and selected photo remain local until the person confirms publication. */
class ProfileCoordinator(private val client: ClientController, private val paths: () -> PrivateFiles?) {
    val saved = ReadStore<PublicProfile>()
    val publication = ReadStore<ProfilePublication>()
    private val formValue = MutableStateFlow(ProfileDraft())
    val form = formValue.asStateFlow()
    private val bitmapValue = MutableStateFlow<Bitmap?>(null)
    val bitmap = bitmapValue.asStateFlow()
    private val conflictValue = MutableStateFlow(false)
    val conflict = conflictValue.asStateFlow()
    private val errorValue = MutableStateFlow<String?>(null)
    val error = errorValue.asStateFlow()
    private var expected: Long? = null
    private var edited = false
    private var photo: ByteArray? = null
    private var uploaded: String? = null
    private var picker: SessionStamp? = null
    private var uncertain = false
    fun edit(value: ProfileDraft) { client.activity(); edited = true; formValue.value = value; errorValue.value = null }
    fun load(force: Boolean = false, rebase: Boolean = false) {
        val request = saved.begin()
        fun apply(value: PublicProfile, fresh: Boolean = false) {
            saved.accept(request, value)
            if (!edited) { formValue.value = value.draft(); expected = value.updatedAt; loadAvatar(value.avatar) }
            else if (rebase && fresh) { expected = value.updatedAt; conflictValue.value = false }
            client.state.value.session?.account?.let { client.rememberAccountName(it, value.name) }
        }
        client.perform({ parseProfileSnapshot(it.cachedDisplay("profile-editor", "").decode()) }, key = "profile:cached", replace = true, quiet = true, onError = { saved.fail(request, it) }) { value ->
            value?.let { apply(it) }
            client.perform({ core -> parseProfileSnapshot(client.operation { core.refreshDisplay("profile-editor", "", force, it) }.decode()) ?: error("INTEGRITY_FAILED") }, key = "profile:read", replace = true, quiet = true, onError = { saved.fail(request, it) }) { apply(it, true) }
        }
        val selectedPhoto = photo?.copyOf()
        if (selectedPhoto != null) client.perform({ try { BoundedImages.thumbnail(Base64.encodeToString(selectedPhoto, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING), 512) } finally { selectedPhoto.fill(0) } }, key = "profile:selected-preview", replace = true, quiet = true) { bitmapValue.value = it }
        else if (edited && !formValue.value.avatarChanged) loadAvatar(formValue.value.avatar)
        client.account.devices()
        status()
    }
    fun status() {
        val request = publication.begin()
        client.perform({ parseProfilePublication(it.profilePublication().decode()) }, key = "profile:publication", replace = true, quiet = true, onError = { publication.fail(request, it) }) {
            publication.accept(request, it)
            if (!it.pending && it.published && !client.isWorking("profile:publish")) uncertain = false
        }
    }
    fun ready() = client.account.devices.value()?.canManage == true && expected != null && formValue.value.valid && edited && !conflictValue.value && !uncertain && publication.value()?.let { it.published && !it.pending } == true
    private fun loadAvatar(url: String?) {
        if (url == null) { bitmapValue.value = null; return }
        client.perform({ core ->
            val bytes = core.cachedProfileAvatar(url) ?: core.profileAvatar(url)
            bytes?.let { try { BoundedImages.thumbnail(Base64.encodeToString(it, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING), 512) } finally { it.fill(0) } }
        }, key = "profile:avatar", replace = true, quiet = true) { if (!formValue.value.avatarChanged && formValue.value.avatar == url) bitmapValue.value = it }
    }
    fun beginPhoto(): Boolean {
        val stamp = client.currentStamp() ?: return false
        if (!client.accepts(stamp) || picker != null || uncertain || client.isWorking("profile:publish")) return false
        picker = stamp; client.beginExternalInteraction(); return true
    }
    fun photoResult(uri: Uri?) {
        val stamp = picker ?: return; picker = null
        if (uri == null) return
        val paths = paths() ?: return
        val account = stamp.account ?: return
        client.deliverExternalResult("profile-photo", stamp) {
            client.perform({
                val handle = paths.stage(uri, account, 32L * 1024 * 1024) { client.sessionCurrent(stamp) }
                try {
                    val bytes = ProfileImages.prepare(File(paths.staging, handle))
                    val image = BoundedImages.thumbnail(Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING), 512)
                    bytes to image
                } finally { paths.finishImport(handle) }
            }, key = "profile:prepare-photo", replace = true, quiet = true, onError = { errorValue.value = it }) { (bytes, image) ->
                photo?.fill(0); photo = bytes; uploaded = null; bitmapValue.value = image
                edit(formValue.value.copy(avatarChanged = true))
            }
        }
    }
    fun removePhoto() { photo?.fill(0); photo = null; uploaded = null; bitmapValue.value = null; edit(formValue.value.copy(avatar = null, avatarChanged = true)) }
    fun publish() {
        val version = expected ?: return
        if (!ready()) return
        val submitted = formValue.value
        uncertain = true; errorValue.value = null
        val image = photo?.copyOf()
        val alreadyUploaded = uploaded
        val stamp = client.currentStamp() ?: return
        client.perform({ core ->
            try {
                val url = if (image != null) alreadyUploaded ?: client.operation { core.uploadProfileAvatar(image, it) } else null
                if (url != null) withContext(Dispatchers.Main.immediate) { if (client.accepts(stamp)) uploaded = url }
                val result = parseProfilePublication(core.publishProfile(submitted.name, submitted.about, version, if (submitted.avatarChanged) url ?: submitted.avatar.orEmpty() else null, submitted.linksJson()).decode())
                result to url
            } finally { image?.fill(0) }
        }, key = "profile:publish", quiet = true, onError = {
            errorValue.value = it
            if (it == "VERSION_CONFLICT") conflictValue.value = true
            // Re-read the native account journal before allowing another edited request.
            status()
        }) { (result, url) ->
            uncertain = false
            val request = publication.begin(); publication.accept(request, result)
            if (formValue.value == submitted) {
                edited = false; expected = result.updatedAt ?: expected
                formValue.value = submitted.copy(avatar = url ?: submitted.avatar, avatarChanged = false)
                photo?.fill(0); photo = null; uploaded = null
            }
            load(true)
        }
    }
    fun retryPublication() {
        uncertain = true
        client.perform({ parseProfilePublication(it.retryProfilePublication().decode()) }, key = "profile:publish", quiet = true, onError = { errorValue.value = it; status() }) {
            uncertain = false; val request = publication.begin(); publication.accept(request, it); load(true)
        }
    }
    fun clear(retainForm: Boolean = false) {
        saved.reset(); publication.reset(); bitmapValue.value = null
        if (!retainForm) { photo?.fill(0); photo = null; uploaded = null; picker = null; expected = null; edited = false; uncertain = false; formValue.value = ProfileDraft(); conflictValue.value = false; errorValue.value = null }
    }
}
