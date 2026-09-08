<script lang="ts">
  import { t } from '$lib/i18n'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { oisyWallet } from '$lib/stores/oisy'
  import { toastRun } from '$lib/stores/toast'
  import { TokenAmount, type TokenInfo } from '$lib/utils/token'
  import type { IcrcAccount } from '@dfinity/oisy-wallet-signer'
  import type { Principal } from '@icp-sdk/core/principal'
  import { getToastStore } from '$lib/ui/stores'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    token: TokenInfo
    to: Principal
    onCompleted: () => Promise<void>
  }

  let { parent, to, token, onCompleted }: Props = $props()

  const toastStore = getToastStore()
  let loading = $state(true)
  let validating = $state(false)
  let submitting = $state(false)
  let topupAmount = $state(0)
  let account: IcrcAccount | null = $state(null)

  function validateAmount(e: Event) {
    const input = e.target as HTMLInputElement

    validating = false
    if (topupAmount < 1) {
      input.setCustomValidity(
        $t('Amount must be greater than 1 {symbol}', { symbol: token.symbol })
      )
      return
    }

    if (input.value.startsWith('0')) {
      input.value = topupAmount.toString()
    }

    input.setCustomValidity('')
    validating = true
  }

  function onTopup(e: Event) {
    submitting = true
    toastRun(async function () {
      const ta = TokenAmount.fromNumber({ amount: topupAmount, token })
      await oisyWallet.transfer(to, token.canisterId, ta.toUlps())
      await onCompleted()
    }, toastStore).finally(() => {
      parent['onClose'] && parent['onClose']()
    })
  }

  onMount(() => {
    const { abort } = toastRun(async function () {
      account = await oisyWallet.connect()
      loading = false
    }, toastStore)
    return abort
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold"
    >{$t('Topup {symbol} from OISY', { symbol: token.symbol })}</div
  >

  <form class="m-auto !mt-4 flex flex-col content-center">
    {#if loading}
      <div class="m-auto">
        <Loading />
      </div>
    {:else if account}
      <div class="space-y-2">
        <p>{$t('From:')} {account.owner}</p>
        <p>{$t('To:')} {to.toText()}</p>
      </div>
    {/if}
    <div class="relative mt-4">
      <input
        class="border-gray/10 peer input valid:input-success truncate rounded-xl bg-white/20"
        type="number"
        name="amount"
        min="0"
        step="any"
        bind:value={topupAmount}
        oninput={validateAmount}
        placeholder={$t('Enter amount')}
        disabled={loading || submitting}
        required
      />
    </div>
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full"
      disabled={loading || submitting || !validating}
      onclick={onTopup}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>{$t('Processing...')}</span>
      {:else}
        <span>{$t('Topup')}</span>
      {/if}
    </button>
  </footer>
</ModalCard>
