package net.dmsg.android.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.ImeAction
import net.dmsg.android.R
import net.dmsg.android.platform.str

@Composable internal fun WorkspaceSearchField(value: String, change: (String) -> Unit, placeholder: String, modifier: Modifier = Modifier, onSearch: (() -> Unit)? = null) {
    val colors = MaterialTheme.colorScheme
    OutlinedTextField(value, change, modifier.fillMaxWidth().semantics { contentDescription = placeholder },
        singleLine = true, shape = CircleShape, placeholder = { Text(placeholder, Modifier.clearAndSetSemantics {}, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        keyboardActions = if (onSearch == null) KeyboardActions.Default else KeyboardActions(onSearch = { onSearch() }),
        colors = OutlinedTextFieldDefaults.colors(focusedContainerColor = colors.surface, unfocusedContainerColor = colors.surface,
            unfocusedBorderColor = colors.outlineVariant, focusedBorderColor = colors.primary),
        leadingIcon = { Icon(Icons.Outlined.Search, null) },
        trailingIcon = if (value.isNotEmpty()) {
            { IconButton(onClick = { change("") }) { Icon(Icons.Outlined.Close, str(R.string.text_clear_search_3f4dec6)) } }
        } else null)
}
