<script lang="ts">
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { authStore } from '$lib/stores/auth'
  import { toastRun } from '$lib/stores/toast'
  import { shortId } from '$lib/utils/auth'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    username: string
  }

  let { parent, username }: Props = $props()

  const toastStore = getToastStore()

  let submitting = $state(false)
  let activated = $state(false)
  let usernameAccount = $state('')

  function onActivate() {
    submitting = true
    toastRun(async () => {
      if (activated) {
        await authStore.switch(username)
      } else {
        await authStore.nameIdentityAPI.activate_name(username)
        activated = true
      }
    }, toastStore).finally(() => {
      submitting = false
    })
  }

  onMount(async () => {
    usernameAccount = (
      await authStore.nameIdentityAPI.get_principal(username)
    ).toText()
  })
</script>

<ModalCard {parent}>
  <div class="text-surface-900-50-token !mt-0 text-center text-xl font-bold"
    >Activate username account</div
  >
  {#if activated}
    <div class="!mt-4 space-y-2 rounded-xl">
      <p>
        <b>1.</b> Your permanent account generated from
        <span class="font-semibold text-primary-500">{username}</span>
        is: <span class="font-semibold">{usernameAccount}</span>.
      </p>
      <p>
        <b>2.</b> You should transfer the username to this account and switch to
        it for management.
      </p>
    </div>
  {:else}
    <div class="!mt-4 space-y-2 rounded-xl">
      <p>
        <b>1.</b> Your permanent account generated from
        <span class="font-semibold text-primary-500">{username}</span>
        is: <span class="font-semibold">{usernameAccount}</span>.
      </p>
      <p>
        <b>2.</b> This account allows you to add multiple delegate accounts, enabling
        team members to use it at the same time—ideal for collaboration.
      </p>
      <p>
        <b>3.</b> Once activated, transfer your username to this account and switch
        to it for management.
      </p>
      <p>
        <b>4.</b> After transferring your username to this account, it will be permanently
        bound and cannot be transferred again.
      </p>
    </div>
  {/if}
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full"
      disabled={submitting}
      onclick={onActivate}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span
          >{activated
            ? 'Switch to ' + shortId(usernameAccount)
            : 'Activate Now'}</span
        >
      {/if}
    </button>
  </footer>
</ModalCard>
