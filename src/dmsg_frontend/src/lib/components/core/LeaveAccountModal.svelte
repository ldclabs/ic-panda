<script lang="ts">
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type SvelteComponent } from 'svelte'

  interface Props {
    parent: SvelteComponent
    username: string
    onConfirm: () => Promise<void>
  }

  let { parent, username, onConfirm }: Props = $props()

  let submitting = $state(false)
  async function onConfirmHandler() {
    submitting = true
    await onConfirm()
    parent && parent['onClose']()
  }
</script>

<ModalCard {parent}>
  <div class="text-surface-900-50-token !mt-0 text-center text-xl font-bold"
    >Leave username account</div
  >
  <div class="!mt-4 space-y-2 rounded-xl">
    <p>
      Are you sure you wish to leave account
      <span class="font-semibold text-primary-500">{username}</span>?
    </p>
  </div>

  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full"
      disabled={submitting}
      onclick={onConfirmHandler}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>Confirm</span>
      {/if}
    </button>
  </footer>
</ModalCard>
