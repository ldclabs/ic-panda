package net.dmsg.android.platform

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Bundle
import android.util.SizeF
import android.widget.RemoteViews
import net.dmsg.android.MainActivity
import net.dmsg.android.R
import net.dmsg.android.core.WorkspaceLink

/** Public navigation only; no workspace, account cache, key or Client access. */
object WorkspaceEntries {
    val labels = linkedMapOf("vault" to R.string.shortcut_vault, "messages" to R.string.shortcut_messages, "files" to R.string.shortcut_files, "approvals" to R.string.shortcut_approvals, "imports" to R.string.shortcut_imports)
    fun intent(context: Context, destination: String) = Intent(Intent.ACTION_VIEW, Uri.parse(WorkspaceLink(destination).url()), context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    private fun shortcut(context: Context, destination: String) = ShortcutInfo.Builder(context, "workspace-" + destination)
        .setShortLabel(AppLanguage.wrap(context).getString(labels.getValue(destination))).setIcon(Icon.createWithResource(context, R.drawable.dmsg_mark))
        .setIntent(intent(context, destination)).build()
    fun updateShortcuts(context: Context) {
        val manager = context.getSystemService(ShortcutManager::class.java)
        if (!manager.isRateLimitingActive) manager.dynamicShortcuts = labels.keys.take(manager.maxShortcutCountPerActivity).map { shortcut(context, it) }
    }
    fun pinShortcut(context: Context, destination: String): Boolean {
        val manager = context.getSystemService(ShortcutManager::class.java)
        return manager.isRequestPinShortcutSupported && manager.requestPinShortcut(shortcut(context, destination), null)
    }
    fun pinWidget(context: Context): Boolean {
        val manager = context.getSystemService(AppWidgetManager::class.java)
        return manager.isRequestPinAppWidgetSupported && manager.requestPinAppWidget(ComponentName(context, WorkspaceWidget::class.java), null, null)
    }
    fun widget(context: Context, wide: Boolean): RemoteViews {
        val views = RemoteViews(context.packageName, if (wide) R.layout.widget_workspace_wide else R.layout.widget_workspace_small)
        val targets = if (wide) linkedMapOf(R.id.widget_vault to "vault", R.id.widget_messages to "messages", R.id.widget_files to "files", R.id.widget_approvals to "approvals", R.id.widget_imports to "imports") else mapOf(R.id.widget_open to "vault")
        targets.entries.forEachIndexed { index, (view, destination) ->
            val pending = PendingIntent.getActivity(context, index + 100, intent(context, destination), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            views.setOnClickPendingIntent(view, pending)
            if (wide) views.setTextViewText(view, AppLanguage.wrap(context).getString(labels.getValue(destination)))
        }
        if (!wide) views.setTextViewText(R.id.widget_open_label, AppLanguage.wrap(context).getString(R.string.widget_open))
        return views
    }
    fun refreshWidgets(context: Context) {
        val manager = AppWidgetManager.getInstance(context)
        val ids = manager.getAppWidgetIds(ComponentName(context, WorkspaceWidget::class.java))
        if (ids.isNotEmpty()) manager.updateAppWidget(ids, responsiveWidget(context))
    }
    fun responsiveWidget(context: Context) = RemoteViews(mapOf(SizeF(120f, 120f) to widget(context, false), SizeF(250f, 160f) to widget(context, true)))
}

class WorkspaceWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        val views = WorkspaceEntries.responsiveWidget(context)
        ids.forEach { manager.updateAppWidget(it, views) }
    }
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, id: Int, options: Bundle) { onUpdate(context, manager, intArrayOf(id)) }
}
