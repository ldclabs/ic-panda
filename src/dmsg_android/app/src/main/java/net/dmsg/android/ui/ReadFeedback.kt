package net.dmsg.android.ui

import net.dmsg.android.ui.kit.*
import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import net.dmsg.android.core.ReadState

@Composable fun <T> ReadFeedback(state: ReadState<T>, retry: () -> Unit) {
    // The first read shows the shape of the content; a refresh keeps the content and a thin bar.
    if (state == ReadState.Idle || state is ReadState.Loading && state.previous == null) SkeletonRows()
    // The bar and the banners unfold and fold while the page is open, so the content moves smoothly.
    Reveal(state is ReadState.Loading && state.previous != null) { LinearProgressIndicator(Modifier.fillMaxWidth()) }
    val failed = retained(state as? ReadState.Failed<*>)
    Reveal(state is ReadState.Failed) {
        failed?.let { Notice(errorText(it.code), warning = true) {
            TextButton(onClick = retry, colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onErrorContainer)) { Text(str(R.string.text_try_again_672e778)) }
        } }
    }
    val stale = retained(state as? ReadState.Stale<*>)
    Reveal(state is ReadState.Stale) {
        stale?.let { Notice(str(R.string.text_showing_the_last_verified_copy_616cefe) + errorText(it.reason)) {
            TextButton(onClick = retry) { Text(str(R.string.text_refresh_cba212b)) }
        } }
    }
}

/** Adjacent reads can fail for the same reason. One retry should recover both resources. */
@Composable internal fun ReadFeedback(first: ReadState<*>, second: ReadState<*>, retryFirst: () -> Unit, retrySecond: () -> Unit) {
    val sameFeedback = when (first) {
        ReadState.Idle, is ReadState.Loading -> second is ReadState.Idle || second is ReadState.Loading
        is ReadState.Failed -> second is ReadState.Failed && first.code == second.code
        is ReadState.Stale -> second is ReadState.Stale && first.reason == second.reason
        else -> false
    }
    if (sameFeedback) ReadFeedback(first) { retryFirst(); retrySecond() }
    else {
        ReadFeedback(first, retryFirst)
        ReadFeedback(second, retrySecond)
    }
}
