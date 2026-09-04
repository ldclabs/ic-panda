<script lang="ts">
  import { Avatar } from 'bits-ui'

  /**
   * Drop-in replacement for Skeleton's `Avatar`, built on the bits-ui
   * primitive. The prop names and defaults are kept so the ~20 call sites
   * did not have to change: an image when there is one, two initials drawn
   * as SVG text when there is not.
   */
  interface Props {
    class?: string
    initials?: string
    fill?: string
    fontSize?: number
    src?: string
    fallback?: string
    background?: string
    width?: string
    border?: string
    rounded?: string
    shadow?: string
    cursor?: string
    alt?: string
    style?: string
    children?: import('svelte').Snippet
  }

  let {
    class: selfClass = '',
    initials = '',
    fill = 'fill-token',
    fontSize = 150,
    src = '',
    fallback = '',
    background = 'bg-surface-400-500-token',
    width = 'w-16',
    border = '',
    rounded = 'rounded-full',
    shadow = '',
    cursor = '',
    alt = '',
    style = '',
    children
  }: Props = $props()

  const base =
    'flex aspect-square text-surface-50 font-semibold justify-center items-center overflow-hidden isolate'

  // bits-ui only swaps to the fallback once the image errors or is absent, so
  // an empty `src` has to be treated as "no image" up front.
  const hasImage = $derived(Boolean(src || fallback))
</script>

<Avatar.Root
  data-testid="avatar"
  class="avatar {base} {background} {width} {border} {rounded} {shadow} {cursor} {selfClass}"
>
  {#if hasImage}
    <Avatar.Image
      class="avatar-image w-full object-cover"
      src={src || fallback}
      {alt}
      {style}
    />
    <Avatar.Fallback class="flex size-full items-center justify-center">
      {#if fallback && src && src !== fallback}
        <img class="w-full object-cover" src={fallback} {alt} />
      {:else if initials}
        <svg class="avatar-initials size-full" viewBox="0 0 512 512">
          <text
            x="50%"
            y="50%"
            dominant-baseline="central"
            text-anchor="middle"
            font-weight="bold"
            font-size={fontSize}
            class="avatar-text {fill}"
          >
            {String(initials).substring(0, 2).toUpperCase()}
          </text>
        </svg>
      {/if}
    </Avatar.Fallback>
  {:else if initials}
    <svg class="avatar-initials size-full" viewBox="0 0 512 512">
      <text
        x="50%"
        y="50%"
        dominant-baseline="central"
        text-anchor="middle"
        font-weight="bold"
        font-size={fontSize}
        class="avatar-text {fill}"
      >
        {String(initials).substring(0, 2).toUpperCase()}
      </text>
    </svg>
  {:else}
    {@render children?.()}
  {/if}
</Avatar.Root>
