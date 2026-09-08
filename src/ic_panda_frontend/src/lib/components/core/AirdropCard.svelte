<script lang="ts">
  import { t, locale } from '$lib/i18n'
  import { goto } from '$app/navigation'
  import { page } from '$app/state'
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import IconAlarmWarning from '$lib/components/icons/IconAlarmWarning.svelte'
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import IconInfo from '$lib/components/icons/IconInfo.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { getModalStore } from '$lib/ui/stores'
  import AirdropModal from './AirdropModal.svelte'
  import LuckyTransferModal from './LuckyTransferModal.svelte'

  const modalStore = getModalStore()
  const luckyPoolState = luckyPoolAPI.stateStore
  const airdropState = luckyPoolAPI.airdropStateStore

  let totalBalance = 0n
  let claimableAmount = 0n
  let claimedAmount = 0n
  let luckyCode = ''
  let showBalanceTip = false

  function claimNowHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: AirdropModal }
      })
    }
  }

  function transferHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: { ref: LuckyTransferModal }
      })
    }
  }

  $: principal = $authStore.identity.getPrincipal()
  $: {
    totalBalance = $luckyPoolState?.airdrop_balance || 0n
    claimableAmount = $airdropState?.claimable || 0n
    claimedAmount = $airdropState?.claimed || 0n
    luckyCode = $airdropState?.lucky_code[0] || ''

    if (luckyCode && page.url?.searchParams.get('ref')) {
      const query = page.url.searchParams
      query.delete('ref')
      goto(`?${query.toString()}`)
    }
  }
</script>

<div
  class="flex flex-col justify-center rounded-2xl bg-white bg-[url('/_assets/images/lucky-pool-bg.webp')] bg-[length:100%_auto] bg-no-repeat p-4"
>
  <section class="mt-5 mb-10 flex flex-col justify-center">
    <h5 class="h5 text-center font-extrabold">
      <span>{$t('Free PANDA Airdrop')}</span>
    </h5>
    <div class="m-auto mt-5 flex flex-row gap-4">
      <div
        class="*:rounded-full *:transition *:duration-700 *:ease-in-out *:hover:scale-125 *:hover:shadow-lg"
      >
        <IconGoldPanda />
      </div>
      <div>
        <h2 class="h2 text-gold font-extrabold">
          {$locale && formatNumber(Number(totalBalance / PANDAToken.one))}
        </h2>
        <button
          class="text-gray/50 mt-2 flex flex-row items-center gap-1"
          aria-expanded={showBalanceTip}
          on:click={() => (showBalanceTip = !showBalanceTip)}
        >
          <span>{$t('Current available balance')}</span>
          <span>
            <IconInfo />
          </span>
        </button>
        {#if showBalanceTip}
          <div
            class="card bg-surface-800 mt-2 max-w-80 px-3 py-2 text-sm text-white"
          >
            <p class="min-w-0 text-balance break-words">
              {$t(
                'We will gradually increase the number of PANDA tokens available for airdrop to ensure an orderly distribution.'
              )}
            </p>
          </div>
        {/if}
      </div>
    </div>
  </section>
  <footer class="m-auto mb-6">
    {#if luckyCode == ''}
      <!-- Anonymous -->
      <p class="text-gray/50 text-sm"
        >{$t('Please read the rules before claiming:')}</p
      >
      <ol class="list *:mt-3">
        <li>
          <span class="badge-icon bg-pink-500 p-2 text-white">1</span>
          <span class="flex-auto">
            {$t(
              'New users can get {amount} PANDA, or {bonus} PANDA with a LUCKY CODE.',
              {
                amount: formatNumber(Number(claimableAmount / PANDAToken.one)),
                bonus: formatNumber(
                  Number(
                    (claimableAmount + claimableAmount / 2n) / PANDAToken.one
                  )
                )
              }
            )}
          </span>
        </li>
        <li>
          <span class="badge-icon bg-pink-500 p-2 text-white">2</span>
          <span class="flex-auto">
            {$t(
              'Your LUCKY CODE will be generated after claiming the airdrop.'
            )}
          </span>
        </li>
        <li>
          <span class="badge-icon bg-pink-500 p-2 text-white">3</span>
          <span class="flex-auto">
            {$t(
              'For each successful referral with your LUCKY CODE, you gain an additional {amount} PANDA.',
              {
                amount: formatNumber(
                  Number(claimableAmount / (2n * PANDAToken.one))
                )
              }
            )}
          </span>
        </li>
      </ol>
      <div class="mt-10 flex flex-col items-center">
        <p
          class="flex flex-row content-center items-center gap-2 text-sm font-medium text-pink-500"
        >
          <span class="*:size-5"><IconAlarmWarning /></span><span
            >{$t('Each user can only claim ONCE.')}</span
          >
        </p>
        <button
          disabled={claimableAmount === 0n ||
            totalBalance < claimableAmount + PANDAToken.fee}
          on:click={claimNowHandler}
          class="btn md:btn-lg m-auto mt-3 w-[320px] max-w-full bg-pink-500 font-medium text-white transition duration-700 ease-in-out hover:scale-110 hover:shadow"
        >
          {$t('Understand and Claim Now')}
        </button>
      </div>
    {:else if luckyCode == 'AAAAAA'}
      <!-- banned user -->
      <p class="">
        <span>{$t('Sorry, you can not claim the airdrop.')}</span>
      </p>
      <button
        disabled={true}
        class="variant-filled-primary btn md:btn-lg m-auto mt-3 flex w-[320px] max-w-full flex-row items-center gap-2 text-white transition duration-700 ease-in-out hover:scale-110 hover:shadow"
      >
        {$t('Got It')}
      </button>
    {:else}
      <p class="">
        <span>
          {$t('The more lucky balance you hold, the bigger prize you grab.')}
        </span>
      </p>
      <p class="mt-3">
        <span><b>{$t('Lucky Balance:')}</b></span>
        <span>
          <b class="text-panda"
            >{$locale &&
              formatNumber(Number(claimableAmount) / Number(PANDAToken.one))}</b
          >
          {$t('PANDA tokens')}
        </span>
        <span>
          ({$locale &&
            formatNumber(Number(claimedAmount) / Number(PANDAToken.one))}
          {$t('tokens transferred out)')}
        </span>
      </p>
      <p class="mt-3">
        <span>{$t('Lucky Code:')}</span>
        <span class="text-panda"><b>{luckyCode}</b></span>
        <TextClipboardButton textValue={luckyCode} />
      </p>
      <p class="mt-3">
        <span>{$t('Link:')}</span>
        <span>
          {`${APP_ORIGIN}/?ref=${luckyCode}`}
        </span>
        <TextClipboardButton textValue={`${APP_ORIGIN}/?ref=${luckyCode}`} />
      </p>
      <button
        disabled={claimableAmount === 0n}
        on:click={transferHandler}
        class="variant-filled-primary btn md:btn-lg m-auto mt-10 flex w-[320px] max-w-full flex-row items-center gap-2 text-white transition duration-700 ease-in-out hover:scale-110 hover:shadow"
      >
        {#if claimableAmount > 0n}
          <span>{$t('Transfer tokens to wallet')}</span>
        {:else}
          <span>{$t('No token to transfer')}</span>
        {/if}
      </button>
    {/if}
  </footer>
</div>
