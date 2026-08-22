<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    /** Stagger in ms, applied as a transition-delay. */
    delay?: number
    class?: string
    children?: Snippet
  }

  let { delay = 0, class: klass = '', children }: Props = $props()

  let el: HTMLDivElement | undefined = $state()
  let visible = $state(false)

  /**
   * Reveals once and then stops observing. Only a class is toggled, so nothing
   * remounts while scrolling.
   */
  $effect(() => {
    const node = el
    if (!node) return

    if (typeof IntersectionObserver === 'undefined') {
      visible = true
      return
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) {
          visible = true
          observer.disconnect()
        }
      },
      { rootMargin: '0px 0px -8% 0px', threshold: 0.05 }
    )

    observer.observe(node)
    return () => observer.disconnect()
  })
</script>

<div
  bind:this={el}
  class="reveal {klass}"
  class:is-visible={visible}
  style:transition-delay={delay ? `${delay}ms` : undefined}
>
  {@render children?.()}
</div>
