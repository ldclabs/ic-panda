package net.dmsg.android.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat
import net.dmsg.android.platform.AppLanguage

internal val LocalDarkTheme = staticCompositionLocalOf { false }

/** Aligns the current window's (Activity or full-screen Dialog) system bar icons with the app theme. */
@Composable internal fun SyncSystemBars(darkTheme: Boolean = LocalDarkTheme.current) {
    val view = LocalView.current
    SideEffect {
        val window = (view.parent as? DialogWindowProvider)?.window ?: run {
            var context: Context? = view.context
            while (context is ContextWrapper && context !is Activity) context = context.baseContext
            (context as? Activity)?.window
        }
        window?.let {
            WindowCompat.getInsetsController(it, view).apply {
                isAppearanceLightStatusBars = !darkTheme
                isAppearanceLightNavigationBars = !darkTheme
            }
        }
    }
}

@Composable fun DMsgTheme(
    isDark: Boolean?,
    dynamic: Boolean = false,
    lightSystemBarIcons: Boolean = false,
    content: @Composable () -> Unit,
) {
    val darkTheme = isDark ?: isSystemInDarkTheme()
    SyncSystemBars(darkTheme || lightSystemBarIcons)
    val view = LocalView.current
    val locale = AppLanguage.locale()
    val typography = remember(locale) { dmsgTypography(locale) }
    val colors = remember(darkTheme, dynamic, LocalConfiguration.current) { appColors(view.context, darkTheme, dynamic) }
    CompositionLocalProvider(LocalDarkTheme provides darkTheme) {
        MaterialTheme(colorScheme = colors, shapes = DMsgShapes,
            typography = typography, content = content)
    }
}

/** Content laid on card paper: components that default to `surface`, such as ListItem and OutlinedCard, take the card colour. */
@Composable internal fun OnCard(content: @Composable () -> Unit) {
    val colors = MaterialTheme.colorScheme
    MaterialTheme(colorScheme = remember(colors) { colors.copy(surface = colors.card) }, content = content)
}
