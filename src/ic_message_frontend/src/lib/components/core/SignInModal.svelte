<script lang="ts">
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
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

<ModalCard {parent} cardClass="backdrop-blur-sm bg-white/30">
  <div class="!mt-0 text-center text-xl font-bold text-white">Sign In with</div>
  <div class="!mt-8 flex flex-col items-center space-y-6">
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
      <span class="mr-2 *:size-6 *:scale-110"><IconIcLogo /></span>
      <span>Internet Identity</span>
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
      <span>identity.ic0.app (legacy)</span>
    </button>
  </div>
</ModalCard>
