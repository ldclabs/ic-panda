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
    when (state) {
        // The first read shows the shape of the content; a refresh keeps the content and a thin bar.
        ReadState.Idle -> SkeletonRows()
        is ReadState.Loading -> if (state.previous == null) SkeletonRows() else LinearProgressIndicator(Modifier.fillMaxWidth())
        is ReadState.Failed -> Notice(errorText(state.code), warning = true) {
            TextButton(onClick = retry, colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onErrorContainer)) { Text(str(R.string.text_try_again_672e778)) }
        }
        is ReadState.Stale -> Notice(str(R.string.text_showing_the_last_verified_copy_616cefe) + errorText(state.reason)) {
            TextButton(onClick = retry) { Text(str(R.string.text_refresh_cba212b)) }
        }
        else -> Unit
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
