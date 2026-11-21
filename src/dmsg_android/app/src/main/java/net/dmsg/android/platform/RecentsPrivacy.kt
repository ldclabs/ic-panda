package net.dmsg.android.platform

import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.Drawable
import android.os.Build
import android.view.View
import android.view.ViewTreeObserver
import android.view.WindowManager
import android.view.inspector.WindowInspector
import androidx.activity.ComponentActivity
import androidx.core.graphics.drawable.toDrawable
import androidx.core.view.doOnAttach
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner

/** Protects both the saved task snapshot and launchers that display the app's live windows. */
internal class RecentsPrivacy(private val activity: ComponentActivity) : DefaultLifecycleObserver {
    private val decor = activity.window.decorView
    private val foregrounds = mutableMapOf<View, Drawable?>()
    private val focusListener = ViewTreeObserver.OnWindowFocusChangeListener { update() }
    private var resumed = false
    private var securePanel: View? = null

    fun install() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) activity.setRecentsScreenshotEnabled(false)
        else decor.doOnAttach {
            // Android 12 checks all the activity's windows for FLAG_SECURE when choosing
            // a task snapshot, even before focus loss on screen-off. An invisible secure
            // panel selects the theme snapshot without securing the visible content.
            val params = WindowManager.LayoutParams(1, 1, WindowManager.LayoutParams.TYPE_APPLICATION_PANEL,
                WindowManager.LayoutParams.FLAG_SECURE or WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE, PixelFormat.TRANSPARENT).apply { token = decor.windowToken }
            val panel = View(activity).apply { visibility = View.INVISIBLE }
            activity.windowManager.addView(panel, params)
            securePanel = panel
        }
        activity.lifecycle.addObserver(this)
        update()
    }

    override fun onResume(owner: LifecycleOwner) { resumed = true; update() }
    override fun onPause(owner: LifecycleOwner) { resumed = false; update() }

    override fun onDestroy(owner: LifecycleOwner) {
        foregrounds.keys.toList().forEach(::release)
        securePanel?.let(activity.windowManager::removeViewImmediate)
    }

    private fun release(root: View) {
        root.viewTreeObserver.removeOnWindowFocusChangeListener(focusListener)
        root.foreground = foregrounds.remove(root)
    }

    private fun update() {
        // Dialogs share the activity token. Attached popups use the parent window token.
        val activityToken = (decor.layoutParams as? WindowManager.LayoutParams)?.token
        val windowToken = decor.windowToken
        val roots = WindowInspector.getGlobalWindowViews().filterTo(mutableSetOf(decor)) { root ->
            val token = (root.layoutParams as? WindowManager.LayoutParams)?.token
            (activityToken != null && token == activityToken) || (windowToken != null && root.applicationWindowToken == windowToken)
        }
        (foregrounds.keys - roots).forEach(::release)
        val hide = !resumed || roots.none { it.hasWindowFocus() }
        for (root in roots) {
            if (root !in foregrounds) {
                foregrounds[root] = root.foreground
                root.viewTreeObserver.addOnWindowFocusChangeListener(focusListener)
            }
            val original = foregrounds[root]
            if (!hide) root.foreground = original
            else if (root.foreground === original) root.foreground = Color.BLACK.toDrawable()
        }
    }
}
