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
    val problem = when (state) {
        is ReadState.Failed -> ReadProblem(state.code, warning = true)
        is ReadState.Stale -> ReadProblem(state.reason, warning = false)
        else -> null
    }
    val shown = retained(problem)
    Reveal(problem != null) {
        shown?.let { issue ->
            val message = errorText(issue.code)
            val colors = if (issue.warning) ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onErrorContainer)
                else ButtonDefaults.textButtonColors()
            Notice(if (issue.warning) message else str(R.string.text_showing_the_last_verified_copy_616cefe) + message, warning = issue.warning) {
                TextButton(onClick = retry, enabled = problem != null, colors = colors) {
                    Text(str(if (issue.warning) R.string.text_try_again_672e778 else R.string.text_refresh_cba212b))
                }
            }
        }
    }
}

private data class ReadProblem(val code: String, val warning: Boolean)

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
