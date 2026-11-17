package net.dmsg.android.ui

import net.dmsg.android.R
import net.dmsg.android.platform.str
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import net.dmsg.android.core.ReadState

@Composable fun <T> ReadFeedback(state: ReadState<T>, retry: () -> Unit) {
    when (state) {
        ReadState.Idle, is ReadState.Loading -> LinearProgressIndicator(Modifier.fillMaxWidth())
        is ReadState.Failed -> Column(Modifier.padding(16.dp)) {
            Notice(errorText(state.code), true)
            TextButton(onClick = retry) { Text(str(R.string.text_try_again_672e778)) }
        }
        is ReadState.Stale -> Column(Modifier.padding(vertical = 8.dp)) {
            Notice(str(R.string.text_showing_the_last_verified_copy_616cefe) + errorText(state.reason))
            TextButton(onClick = retry) { Text(str(R.string.text_refresh_cba212b)) }
        }
        else -> Unit
    }
}
