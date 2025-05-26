<script>
  import { onMount } from 'svelte'

  let {
    animation = 'none',
    animation_out = 'none; opacity: 0',
    once = false,
    top = 0,
    bottom = 0,
    css_observer = '',
    css_animation = '',
    children
  } = $props()

  // be aware... he's looking...
  let observing = $state(true)
  let container = $state()

  // 生成唯一的容器 ID
  const containerClass = `__saos-${Math.random().toString(36).slice(2, 9)}__`

  function intersection_verify(box) {
    // bottom left top right
    const rootMargin = `${-bottom}px 0px ${-top}px 0px`

    const observer = new IntersectionObserver(
      (entries) => {
        observing = entries[0].isIntersecting
        if (observing && once) {
          observer.unobserve(box)
        }
      },
      {
        rootMargin
      }
    )

    observer.observe(box)
    return () => observer.unobserve(box)
  }

  /// Fallback in case the browser not have the IntersectionObserver
  function bounding_verify(box) {
    const verify = () => {
      const c = box.getBoundingClientRect()
      observing = c.top + top < window.innerHeight && c.bottom - bottom > 0

      if (observing && once) {
        window.removeEventListener('scroll', verify)
      }
    }

    verify() // 初始检查
    window.addEventListener('scroll', verify)
    return () => window.removeEventListener('scroll', verify)
  }

  onMount(() => {
    if (!container) return

    if (IntersectionObserver) {
      return intersection_verify(container)
    } else {
      return bounding_verify(container)
    }
  })
</script>

<div bind:this={container} class={containerClass} style={css_observer}>
  {#if observing}
    <div style="animation: {animation}; {css_animation}">
      {@render children?.()}
    </div>
  {:else}
    <div style="animation: {animation_out}; {css_animation}">
      {@render children?.()}
    </div>
  {/if}
</div>
