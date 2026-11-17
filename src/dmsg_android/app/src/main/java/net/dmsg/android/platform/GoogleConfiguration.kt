package net.dmsg.android.platform

import android.content.Context
import net.dmsg.android.BuildConfig
import org.json.JSONObject

/** Public Firebase identifiers are fixed by the build; no service-account secret belongs here. */
data class GoogleConfiguration(val packageName: String, val environment: String, val appId: String, val apiKey: String, val projectId: String, val projectNumber: Long) {
    companion object {
        fun load(context: Context): GoogleConfiguration? = parse(BuildConfig.GOOGLE_CONFIG, context.packageName, if (BuildConfig.IS_STAGING) "staging" else "production")
        fun parse(raw: String, packageName: String, environment: String): GoogleConfiguration? = runCatching {
            require(raw.isNotEmpty() && raw.length <= 16384)
            val o = JSONObject(raw)
            require(o.keys().asSequence().toSet() == setOf("package_name", "environment", "firebase_app_id", "api_key", "project_id", "cloud_project_number"))
            val number = o.getString("cloud_project_number").toLong()
            require(number > 0 && number.toString() == o.getString("cloud_project_number"))
            val id = o.getString("firebase_app_id")
            require(id.matches(Regex("1:" + number + ":android:[0-9a-f]+")))
            val project = o.getString("project_id"); require(project.matches(Regex("[a-z][a-z0-9-]{4,61}[a-z0-9]")))
            val key = o.getString("api_key"); require(key.length in 20..256 && key.none(Char::isWhitespace))
            require(o.getString("package_name") == packageName && o.getString("environment") == environment)
            GoogleConfiguration(packageName, environment, id, key, project, number)
        }.getOrNull()
    }
}
