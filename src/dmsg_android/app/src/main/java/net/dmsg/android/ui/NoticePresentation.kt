package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Notifications
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import net.dmsg.android.core.ClientController
import net.dmsg.android.ui.kit.retained

val LocalNoticePermission = staticCompositionLocalOf<() -> Unit> { {} }
@Composable fun rememberNoticePermission(client: ClientController): () -> Unit {
    val context = LocalContext.current
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { client.notices.refresh(receive = true) }
    LaunchedEffect(Unit) { client.notices.system.installChannels() }
    return {
        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            client.beginExternalInteraction(); launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else client.notices.refresh(receive = true)
    }
}

@Composable fun NoticeBanner(client: ClientController) {
    val plan by client.notices.presented.collectAsStateWithLifecycle()
    val state by client.state.collectAsStateWithLifecycle()
    // The card slides down into place and fades out when it goes; covering the screen hides it at once.
    val shown = plan?.takeIf { !state.concealed && !state.starting }
    val current = retained(shown) ?: return
    val drop = with(LocalDensity.current) { 16.dp.roundToPx() }
    AnimatedVisibility(shown != null, Modifier.fillMaxSize(), enter = slideInVertically(Motion.spatial()) { -drop } + fadeIn(Motion.effects()),
        exit = if (state.concealed) ExitTransition.None else fadeOut(Motion.effectsFast())) {
        Box(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).padding(horizontal = 16.dp, vertical = 72.dp)) {
            ElevatedCard(Modifier.align(Alignment.TopCenter).widthIn(max = 560.dp).fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Outlined.Notifications, null)
                        Text(noticeCategoryTitle(current.category), Modifier.weight(1f).padding(horizontal = 12.dp), style = MaterialTheme.typography.titleMedium)
                        IconButton(onClick = client.notices::dismiss) { Icon(Icons.Outlined.Close, str(R.string.text_dismiss_reminder_daba687)) }
                    }
                    Text(client.notices.system.body(current.category, current.generic))
                    TextButton(onClick = client.notices::open) { Text(str(R.string.text_open_reminder_9f594be)) }
                }
            }
        }
    }
}
