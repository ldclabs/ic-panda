package net.dmsg.android.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView
import androidx.core.view.WindowCompat
import net.dmsg.android.platform.AppLanguage

@Composable fun DMsgTheme(isDark: Boolean?, dynamic: Boolean = false, content: @Composable () -> Unit) {
    val darkTheme = isDark ?: isSystemInDarkTheme()
    val view = LocalView.current
    SideEffect {
        var context: Context? = view.context
        while (context is ContextWrapper && context !is Activity) context = context.baseContext
        (context as? Activity)?.window?.let { window ->
            WindowCompat.getInsetsController(window, view).apply {
                isAppearanceLightStatusBars = !darkTheme
                isAppearanceLightNavigationBars = !darkTheme
            }
        }
    }
    val locale = AppLanguage.locale()
    val typography = remember(locale) { dmsgTypography(locale) }
    val colors = remember(darkTheme, dynamic, view.context) { appColors(view.context, darkTheme, dynamic) }
    MaterialTheme(colorScheme = colors, shapes = DMsgShapes,
        typography = typography, content = content)
}

/** Content laid on card paper: components that default to `surface`, such as ListItem and OutlinedCard, take the card colour. */
@Composable internal fun OnCard(content: @Composable () -> Unit) {
    val colors = MaterialTheme.colorScheme
    MaterialTheme(colorScheme = remember(colors) { colors.copy(surface = colors.card) }, content = content)
}
