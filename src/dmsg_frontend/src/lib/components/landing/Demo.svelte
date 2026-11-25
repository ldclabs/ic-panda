<script lang="ts">
  import { replaceState } from '$app/navigation'
  import { page } from '$app/state'
  import { onDestroy, onMount, type Component, type Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'

  // `label` replaces the sample-data notice; an empty label hides it for
  // content that is an explanation rather than a simulation.
  let {
    title,
    id,
    label,
    open = $bindable(false),
    children,
    loadFigure,
    afterFigure
  }: {
    title: string
    id?: string
    label?: string
    open?: boolean
    children?: Snippet
    loadFigure?: () => Promise<{ default: Component }>
    afterFigure?: Snippet
  } = $props()

  let Figure = $state.raw<Component | null>(null)
  let loading = $state(false)
  let loadFailed = $state(false)
  let alive = true

  function reloadFigure() {
    // Reload the entry as well: it may reference a chunk from an older deployment.
    if (id) replaceState(`#${id}`, page.state)
    location.reload()
  }

  $effect(() => {
    if (!open) {
      loadFailed = false
      return
    }
    if (!loadFigure || Figure || loading || loadFailed) return
    loading = true
    loadFigure().then(
      (mod) => {
        if (!alive) return
        Figure = mod.default
        loading = false
      },
      () => {
        if (!alive) return
        loadFailed = true
        loading = false
      }
    )
  })

  // A link to this demo opens it, so the target is never a closed heading.
  onMount(() => {
    if (!id) return
    const reveal = () => {
      if (location.hash === `#${id}`) open = true
    }
    reveal()
    addEventListener('hashchange', reveal)
    return () => removeEventListener('hashchange', reveal)
  })

  onDestroy(() => {
    alive = false
  })
</script>

<details {id} class="deep-dive" bind:open>
  <summary
    ><Glyph name="play" size={18} /><span>{title}</span><Glyph
      name="arrow-right"
      size={18}
    /></summary
  >
  <div class="deep-body">
    {#if label !== ''}
      <p class="demo-label"
        ><Glyph name="eye" size={16} />{label ??
          $t('Interactive example · sample data · nothing is sent')}</p
      >
    {/if}
    {@render children?.()}
    {#if loadFigure}
      {#if Figure}
        <Figure />
      {:else if loadFailed}
        <p role="alert">{$t('Could not load this page.')}</p>
        <button type="button" class="button secondary" onclick={reloadFigure}
          >{$t('Try again')}</button
        >
      {:else if loading}
        <p>{$t('Loading…')}</p>
      {/if}
    {/if}
    {@render afterFigure?.()}
  </div>
</details>
