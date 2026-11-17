package net.dmsg.android.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import net.dmsg.android.R
import net.dmsg.android.platform.str

@Composable internal fun WorkspaceSearchField(value: String, change: (String) -> Unit, placeholder: String, modifier: Modifier = Modifier) {
    OutlinedTextField(value, change, modifier.fillMaxWidth(), singleLine = true,
        shape = RoundedCornerShape(24.dp), placeholder = { Text(placeholder) },
        leadingIcon = { Icon(Icons.Outlined.Search, null) },
        trailingIcon = if (value.isNotEmpty()) {
            { IconButton(onClick = { change("") }) { Icon(Icons.Outlined.Close, str(R.string.text_clear_search_3f4dec6)) } }
        } else null)
}
