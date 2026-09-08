<script lang="ts">
  import { t } from '$lib/i18n'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { authStore } from '$lib/stores/auth'
  import { toastRun } from '$lib/stores/toast'
  import { shortId } from '$lib/utils/auth'
  import { getToastStore } from '$lib/ui/stores'
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
    >{$t('Activate username account')}</div
  >
  {#if activated}
    <div class="!mt-4 space-y-2 rounded-xl">
      <p>
        <b>1.</b>
        {$t('Your permanent account for {username} is {account}.', {
          username,
          account: usernameAccount
        })}
      </p>
      <p>
        <b>2.</b>
        {$t(
          'You should transfer the username to this account and switch to it for management.'
        )}
      </p>
    </div>
  {:else}
    <div class="!mt-4 space-y-2 rounded-xl">
      <p>
        <b>1.</b>
        {$t('Your permanent account for {username} is {account}.', {
          username,
          account: usernameAccount
        })}
      </p>
      <p>
        <b>2.</b>
        {$t(
          'This account allows you to add multiple delegate accounts, enabling team members to use it at the same time—ideal for collaboration.'
        )}
      </p>
      <p>
        <b>3.</b>
        {$t(
          'Once activated, transfer your username to this account and switch to it for management.'
        )}
      </p>
      <p>
        <b>4.</b>
        {$t(
          'After transferring your username to this account, it will be permanently bound and cannot be transferred again.'
        )}
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
        <span>{$t('Processing...')}</span>
      {:else}
        <span
          >{activated
            ? $t('Switch to {account}', { account: shortId(usernameAccount) })
            : $t('Activate Now')}</span
        >
      {/if}
    </button>
  </footer>
</ModalCard>
