<script lang="ts">
  import { glyphs, type GlyphName } from './glyphs'

  let { name, size = 20 }: { name: GlyphName; size?: number } = $props()
</script>

<svg
  class="glyph glyph-{name}"
  viewBox="0 0 24 24"
  width={size}
  height={size}
  aria-hidden="true"
  focusable="false"><path fill="currentColor" d={glyphs[name]} /></svg
>
