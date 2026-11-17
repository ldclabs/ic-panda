package net.dmsg.android.core

/** FCM transport metadata has no account, content, command, URL or signing authority. */
data class RemoteNotice(val route: NoticeRoute, val registration: String, val event: String, val systemId: String, val category: String, val generic: Boolean, val sound: Boolean, val expires: Long) {
    companion object {
        fun parse(data: Map<String, String>, now: Long): RemoteNotice? = runCatching {
            require(data.keys == setOf("version", "route", "registration", "event_id", "system_id", "category", "generic", "sound", "expires"))
            require(data.values.sumOf { it.length } <= 1024 && data["version"] == "1")
            val route = checkNotNull(NoticeRoute.parse(data.getValue("route")))
            require(!route.value.startsWith("local:"))
            val registration = data.getValue("registration"); val event = data.getValue("event_id"); val id = data.getValue("system_id")
            require(validNoticeHash(registration) && validNoticeHash(event) && validNoticeHash(id))
            val category = data.getValue("category"); require(category in noticeCategories)
            val generic = data.getValue("generic"); require(generic == "generic" || generic == category)
            val sound = data.getValue("sound"); require(sound == "true" || sound == "false")
            val expiryText = data.getValue("expires"); val expires = expiryText.toLong()
            require(expiryText == expires.toString() && expires > now && expires <= now + 86_460_000)
            RemoteNotice(route, registration, event, id, category, generic == "generic", sound == "true", expires)
        }.getOrNull()
    }
}
