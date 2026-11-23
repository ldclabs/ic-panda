package net.dmsg.android.ui.kit

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import net.dmsg.android.ui.OnCard
import net.dmsg.android.ui.segmentShape

private val LocalInSection = staticCompositionLocalOf { false }
internal val LocalSectionIconColor = staticCompositionLocalOf { Color.Unspecified }

/**
 * Rows on the card colour, 2 dp apart, as in Android 16 settings: the section keeps the large
 * outer corners and every kit row clips itself to the small segment corners.
 */
@Composable internal fun Section(
    modifier: Modifier = Modifier,
    title: String? = null,
    footer: String? = null,
    iconColor: Color = Color.Unspecified,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier.fillMaxWidth()) {
        title?.let { SectionTitle(it) }
        OnCard {
            CompositionLocalProvider(LocalInSection provides true, LocalSectionIconColor provides iconColor) {
                Column(Modifier.fillMaxWidth().clip(MaterialTheme.shapes.large),
                    verticalArrangement = Arrangement.spacedBy(2.dp), content = content)
            }
        }
        footer?.let {
            Text(it, Modifier.padding(start = 16.dp, top = 8.dp, end = 16.dp), style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable internal fun SectionTitle(text: String) {
    Text(text, Modifier.padding(start = 4.dp, top = 18.dp, bottom = 8.dp).semantics { heading() },
        style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold)
}

/** The shape a row clips to: the segment inside a [Section], none elsewhere. */
@Composable internal fun rowShape(): Shape? = if (LocalInSection.current) segmentShape else null

internal fun Modifier.clipTo(shape: Shape?): Modifier = if (shape == null) this else clip(shape)
