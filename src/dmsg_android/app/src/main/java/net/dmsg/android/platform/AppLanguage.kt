package net.dmsg.android.platform

import android.app.Activity
import android.app.Application
import android.app.LocaleManager
import android.content.Context
import android.content.res.Configuration
import android.os.Build
import android.os.LocaleList
import androidx.annotation.PluralsRes
import androidx.annotation.StringRes
import java.math.BigInteger
import java.text.NumberFormat
import java.util.Locale

/** Installation preference only. Changing language never opens an account. */
object AppLanguage {
    val supported = linkedMapOf(
        "en" to "English", "zh-Hans" to "简体中文", "zh-Hant" to "繁體中文",
        "ar" to "العربية", "bn" to "বাংলা", "de" to "Deutsch", "es" to "Español",
        "fil" to "Filipino", "fr" to "Français", "hi" to "हिन्दी", "id" to "Bahasa Indonesia",
        "it" to "Italiano", "ja" to "日本語", "ko" to "한국어", "ms" to "Bahasa Melayu",
        "nl" to "Nederlands", "pl" to "Polski", "pt-BR" to "Português (Brasil)",
        "ru" to "Русский", "th" to "ไทย", "tr" to "Türkçe", "ur" to "اردو", "vi" to "Tiếng Việt",
    )
    private lateinit var application: Application
    @Volatile private var localizedContext: Context? = null

    fun initialize(app: Application) {
        application = app
        if (Build.VERSION.SDK_INT >= 33) {
            val saved = preferences(app)
            if (!saved.getBoolean("framework-migrated", false)) {
                val legacy = saved.getString("tag", "").orEmpty()
                val manager = app.getSystemService(LocaleManager::class.java)
                if (manager.applicationLocales.isEmpty && legacy in supported) manager.applicationLocales = LocaleList.forLanguageTags(legacy)
                saved.edit().putBoolean("framework-migrated", true).remove("tag").apply()
            }
        }
        configurationChanged()
    }

    private fun preferences(context: Context) = context.getSharedPreferences("application-language", Context.MODE_PRIVATE)

    fun selected(context: Context = application): String = if (Build.VERSION.SDK_INT >= 33) {
        context.getSystemService(LocaleManager::class.java).applicationLocales.toLanguageTags().substringBefore(',')
    } else preferences(context).getString("tag", "").orEmpty()

    /** Android 13 owns the choice; Android 12 uses the same choice in every component. */
    fun wrap(context: Context): Context {
        if (Build.VERSION.SDK_INT >= 33) return context
        val tag = selected(context)
        if (tag !in supported) return context
        val config = Configuration(context.resources.configuration)
        config.setLocales(LocaleList.forLanguageTags(tag))
        return context.createConfigurationContext(config)
    }

    fun choose(activity: Activity, tag: String) {
        require(tag.isEmpty() || tag in supported)
        if (selected(activity) == tag) return
        if (Build.VERSION.SDK_INT >= 33) {
            activity.getSystemService(LocaleManager::class.java).applicationLocales = LocaleList.forLanguageTags(tag)
        } else {
            preferences(activity).edit().putString("tag", tag).apply()
            configurationChanged()
            refreshSystemLabels()
            activity.recreate()
        }
    }

    /** Android 12's Window context does not inherit a wrapped Activity locale. */
    fun applyToWindow(activity: Activity) {
        activity.window.decorView.layoutDirection = activity.resources.configuration.layoutDirection
    }

    fun configurationChanged() {
        localizedContext = null
        // Date/number formatters and the public push locale use the application choice too.
        Locale.setDefault(context().resources.configuration.locales[0])
    }

    fun refreshSystemLabels() {
        runCatching { WorkspaceEntries.updateShortcuts(context()) }
        runCatching { WorkspaceEntries.refreshWidgets(context()) }
        runCatching { SystemNotices(context()).refreshExistingChannelNames() }
    }

    fun context(): Context = if (Build.VERSION.SDK_INT >= 33) application else localizedContext ?: synchronized(this) {
        localizedContext ?: wrap(application).also { localizedContext = it }
    }
    fun locale(): Locale = context().resources.configuration.locales[0]
}

fun str(@StringRes id: Int, vararg arguments: Any): String =
    AppLanguage.context().resources.getString(id, *arguments)

fun localizedNumber(value: ULong): String = NumberFormat.getIntegerInstance(AppLanguage.locale()).format(BigInteger(value.toString()))
fun localizedNumber(value: UInt): String = localizedNumber(value.toULong())
fun localizedNumber(value: Int): String = NumberFormat.getIntegerInstance(AppLanguage.locale()).format(value)
fun localizedNumber(value: Long): String = NumberFormat.getIntegerInstance(AppLanguage.locale()).format(value)

/** A measurement or ratio is read in the same order when embedded in an RTL sentence. */
internal fun numericExpression(value: String): String =
    if (android.text.TextUtils.getLayoutDirectionFromLocale(AppLanguage.locale()) == android.view.View.LAYOUT_DIRECTION_RTL) "\u2066$value\u2069" else value

fun quantity(@PluralsRes id: Int, count: Int): String = quantity(id, count.coerceAtLeast(0).toULong())
fun quantity(@PluralsRes id: Int, count: UInt): String = quantity(id, count.toULong())

fun quantity(@PluralsRes id: Int, count: ULong): String {
    // Keep the CLDR integer suffix and million category without overflowing Android's Int API.
    val selector = if (count <= Int.MAX_VALUE.toULong()) count.toInt() else 1_000_000 + (count % 1_000_000u).toInt()
    return AppLanguage.context().resources.getQuantityString(id, selector, localizedNumber(count))
}
