<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { authStore } from '$lib/stores/auth'
  import { type SvelteComponent } from 'svelte'

  interface Props {
    parent: SvelteComponent
    onCompleted: () => Promise<void>
  }

  let { parent, onCompleted }: Props = $props()

  let submitting = $state(false)
</script>

<ModalCard {parent}>
  <div class="text-surface-900-50-token !mt-0 text-center text-xl font-bold"
    >Sign In with</div
  >
  <div class="!mt-8 flex flex-col items-center space-y-4">
    <button
      type="button"
      class="variant-filled-primary btn w-80"
      disabled={submitting}
      onclick={() => {
        submitting = true
        authStore
          .signIn2()
          .then(() => {
            submitting = false
            onCompleted && onCompleted()
            parent && parent['onClose']()
          })
          .catch(() => {
            submitting = false
          })
      }}
    >
      Internet Identity
    </button>
    <button
      type="button"
      class="variant-filled-secondary btn w-80"
      disabled={submitting}
      onclick={() => {
        submitting = true
        authStore
          .signIn()
          .then(() => {
            submitting = false
            onCompleted && onCompleted()
            parent && parent['onClose']()
          })
          .catch(() => {
            submitting = false
          })
      }}
    >
      identity.ic0.app (legacy)
    </button>
  </div>
</ModalCard>
