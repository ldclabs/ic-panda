<script lang="ts">
  import { t } from '$lib/i18n'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import { type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    imageUrl: string
    imageName: string
  }

  let { parent, imageUrl, imageName }: Props = $props()
</script>

<div class="card bg-surface-50-900-token relative w-dvw">
  <button
    aria-label={$t('Close dialog')}
    class="btn btn-icon absolute top-1 right-1 z-1 text-neutral-500 *:scale-125 hover:scale-110 max-md:top-2 max-md:right-2"
    onclick={parent['onClose']}
  >
    <IconClose />
  </button>
  <img class="m-auto p-4" src={imageUrl} alt={imageName} />
</div>
