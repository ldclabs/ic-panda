<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'

  // `label` replaces the sample-data notice; an empty label hides it for
  // content that is an explanation rather than a simulation.
  let {
    title,
    id,
    label,
    open = $bindable(false),
    children
  }: {
    title: string
    id?: string
    label?: string
    open?: boolean
    children: Snippet
  } = $props()

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
</script>

<details {id} class="deep-dive" bind:open>
  <summary
    ><Glyph name="play" size={18} /><span>{title}</span><Glyph
      name="arrow-right"
      size={18}
    /></summary
  >
  {#if open}
    <div class="deep-body">
      {#if label !== ''}
        <p class="demo-label"
          ><Glyph name="eye" size={16} />{label ??
            $t('Interactive example · sample data · nothing is sent')}</p
        >
      {/if}
      {@render children()}
    </div>
  {/if}
</details>
