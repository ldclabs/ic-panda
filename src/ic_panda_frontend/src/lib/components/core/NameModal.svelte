<script lang="ts">
  import { t, locale } from '$lib/i18n'
  import { luckyPoolAPI, type NameOutput } from '$lib/canisters/luckypool'
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { LUCKYPOOL_CANISTER_ID } from '$lib/constants'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { Principal } from '@icp-sdk/core/principal'
  import { getToastStore } from '$lib/ui/stores'
  import { type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let nameEditMode: 0 | 1 | 2 = 0 // set, update, release
  export let availablePandaBalance = 0n
  export let nameState: Readable<NameOutput | null>

  const NamingDeposit = 3000n * PANDAToken.one

  // const modalStore = getModalStore()
  const toastStore = getToastStore()
  const luckyPoolPrincipal = Principal.fromText(LUCKYPOOL_CANISTER_ID)

  let validating = false
  let submitting = false

  let nameInput = $nameState?.name || ''
  let nameErr = ''
  let result: NameOutput | null = null
  let refund: bigint | null = null

  function checkName() {
    nameErr = ''
    nameInput = nameInput.trim()

    if (
      nameInput == '' ||
      nameInput.includes('\r') ||
      nameInput.includes('\n') ||
      nameInput.includes('\t')
    ) {
      return $t('Enter a name without line breaks')
    }

    if (nameEditMode == 1 && nameInput == $nameState?.name) {
      return $t('The name is the same as the current one')
    }

    return ''
  }

  async function nameCopyPaste(e: Event) {
    e.preventDefault()

    if (nameInput == '') {
      nameInput = await navigator.clipboard.readText()
    } else {
      nameInput = ''
    }
    checkName()
  }

  async function onRegister(e: Event) {
    e.preventDefault()

    submitting = true

    try {
      const res = await luckyPoolAPI.nameLookup(nameInput)
      if (res) {
        nameErr = 'Try another one, this name is occupied'
        submitting = false
        validating = true
        return
      }

      await tokenLedgerAPI.ensureAllowance(
        luckyPoolPrincipal,
        NamingDeposit + PANDAToken.fee
      )
      result = await luckyPoolAPI.registerName(nameInput)
    } catch (err: any) {
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  async function onUpdate(e: Event) {
    e.preventDefault()

    submitting = true

    try {
      if (
        nameInput.toLocaleLowerCase() != $nameState?.name?.toLocaleLowerCase()
      ) {
        const res = await luckyPoolAPI.nameLookup(nameInput)
        if (res) {
          nameErr = 'Try another one, this name is occupied'
          submitting = false
          validating = true
          return
        }
      }

      result = await luckyPoolAPI.updateName(nameInput, $nameState?.name || '')
    } catch (err: any) {
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  async function onUnregister(e: Event) {
    e.preventDefault()

    submitting = true

    try {
      refund = await luckyPoolAPI.unregisterName($nameState?.name || '')
    } catch (err: any) {
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  function onFormChange(e: Event) {
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    const input = form['pandaName'] as HTMLInputElement
    input?.setCustomValidity(checkName())
    validating = form.checkValidity()
  }
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-panda text-center *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>
          {$t(
            nameEditMode == 0
              ? 'You have successfully registered a name:'
              : 'You have successfully updated a name:'
          )}
        </span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{result.name}</p>
      <p class="text-left"
        >{$t('You can update the name for free at any time.')}</p
      >
    </div>
  {:else if refund !== null}
    <div class="text-panda text-center *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>{$t('You have successfully unregistered the name:')}</span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{nameInput}</p>
      <p
        >{$t('{amount} tokens refunded. Check your Wallet Balance.', {
          amount: formatNumber(Number(refund) / Number(PANDAToken.one))
        })}</p
      >
    </div>
  {:else if nameEditMode == 0}
    <div class="!mt-0 text-center text-xl font-bold">{$t('Register Name')}</div>
    <div class="bg-gray/5 space-y-2 rounded-xl p-4">
      <p class="text-gray/50">
        <b>1.</b>
        {$t(
          'To register a name, pay a deposit of {deposit} PANDA. An annual fee of {fee} PANDA is deducted from the deposit. After 10 years, the name is yours permanently.',
          {
            deposit: formatNumber(
              Number(NamingDeposit) / Number(PANDAToken.one)
            ),
            fee: formatNumber(
              Number(NamingDeposit / 10n) / Number(PANDAToken.one)
            )
          }
        )}
      </p>
      <p class="text-gray/50">
        <b>2.</b>
        {$t('You can update the name for free at any time.')}
      </p>
      <p class="text-gray/50">
        <b>3.</b>
        {$t(
          'If you unregister the name early, the remaining deposit after fees is refunded to your lucky balance.'
        )}
      </p>
    </div>
    <hr class="!border-gray/20 mx-[-24px] !mt-6 !border-t-1 !border-dashed" />
    <div class="!mt-5 text-sm">
      <div class="mt-1 flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-2">
          <span class="*:size-6"><IconPanda /></span>
          <b>{$t('Your Wallet Balance:')}</b>
        </div>
        <div class="text-gray/50 flex flex-row gap-1">
          <span
            >{$locale &&
              formatNumber(
                Number(availablePandaBalance) / Number(PANDAToken.one)
              )}</span
          >
          <span>{PANDAToken.symbol}</span>
        </div>
      </div>
    </div>
    <form
      class="m-auto !mt-4 flex flex-col content-center"
      on:input={onFormChange}
    >
      <div class="relative">
        <input
          class="input border-gray/10 invalid:input-warning truncate rounded-xl bg-white/20 pr-16 hover:bg-white/90"
          type="text"
          name="nameInput"
          minlength="2"
          maxlength="48"
          bind:value={nameInput}
          disabled={submitting ||
            availablePandaBalance < 1000n * PANDAToken.one}
          placeholder={$t('Enter a name without line breaks')}
          required
        />
        <button
          aria-label={$t(nameInput ? 'Clear' : 'Paste')}
          class="btn absolute top-0 right-0 outline-0"
          disabled={submitting}
          on:click={nameCopyPaste}
        >
          {#if nameInput == ''}
            <span>{$t('Paste')}</span>
          {:else}
            <span class="*:scale-90"><IconDeleteBin /></span>
          {/if}
        </button>
        <p
          class="h-5 pl-3 text-sm text-red-500 {nameErr == ''
            ? 'invisible'
            : 'visiable'}">{$t(nameErr)}</p
        >
      </div>
    </form>
    <footer class="m-auto !mt-2">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={onRegister}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>{$t('Processing...')}</span>
        {:else}
          <span>{$t('Register Now')}</span>
        {/if}
      </button>
    </footer>
  {:else if nameEditMode == 1}
    <div class="!mt-0 text-center text-xl font-bold">{$t('Update Name')}</div>
    <div class="bg-gray/5 space-y-2 rounded-xl p-4">
      <p class="text-gray/50 mt-4">
        <span>{$t('You are updating the name:')}</span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{$nameState?.name || ''}</p>
      <p class="text-gray/50">{$t('You can update it for free at any time.')}</p
      >
    </div>
    <hr class="!border-gray/20 mx-[-24px] !mt-6 !border-t-1 !border-dashed" />
    <form
      class="m-auto !mt-6 flex flex-col content-center"
      on:input={onFormChange}
    >
      <div class="relative">
        <input
          class="input border-gray/10 invalid:input-warning truncate rounded-xl bg-white/20 pr-16 hover:bg-white/90"
          type="text"
          name="nameInput"
          minlength="2"
          maxlength="48"
          bind:value={nameInput}
          disabled={submitting}
          placeholder={$t('Enter a name without line breaks')}
          required
        />
        <button
          aria-label={$t(nameInput ? 'Clear' : 'Paste')}
          class="btn absolute top-0 right-0 outline-0"
          disabled={submitting}
          on:click={nameCopyPaste}
        >
          {#if nameInput == ''}
            <span>{$t('Paste')}</span>
          {:else}
            <span class="*:scale-90"><IconDeleteBin /></span>
          {/if}
        </button>
        <p
          class="h-5 pl-3 text-sm text-red-500 {nameErr == ''
            ? 'invisible'
            : 'visiable'}">{$t(nameErr)}</p
        >
      </div>
    </form>
    <footer class="m-auto !mt-2">
      <button
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={onUpdate}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>{$t('Processing...')}</span>
        {:else}
          <span>{$t('Update Now')}</span>
        {/if}
      </button>
    </footer>
  {:else if nameEditMode == 2}
    <div class="!mt-0 text-center text-xl font-bold"
      >{$t('Unregister Name')}</div
    >
    <div class="bg-gray/5 space-y-2 rounded-xl p-4">
      <p class="text-gray/50 mt-4">
        <span>{$t('You are unregistering the name:')}</span>
      </p>
      <p class="my-2 text-center text-lg font-bold">{$nameState?.name || ''}</p>
      <p class="text-gray/50">
        <b>1.</b>
        {$t(
          'If you unregister the name early, the remaining deposit after fees is refunded to your lucky balance.'
        )}
      </p>
      <p class="text-gray/50">
        <b>2.</b>
        {$t(
          'To register a name, pay a deposit of {deposit} PANDA. An annual fee of {fee} PANDA is deducted from the deposit. After 10 years, the name is yours permanently.',
          {
            deposit: formatNumber(
              Number(NamingDeposit) / Number(PANDAToken.one)
            ),
            fee: formatNumber(
              Number(NamingDeposit / 10n) / Number(PANDAToken.one)
            )
          }
        )}
      </p>
    </div>
    <hr class="!border-gray/20 mx-[-24px] !mt-6 !border-t-1 !border-dashed" />
    <footer class="m-auto !mt-6">
      <button
        class="variant-filled-warning btn w-full text-white"
        disabled={submitting}
        on:click={onUnregister}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>{$t('Processing...')}</span>
        {:else}
          <span>{$t('Unregister It')}</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
