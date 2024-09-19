<script lang="ts">
  import {
    CKDogeCanisterAPI,
    ckDogeCanisterAPIAsync,
    type State
  } from '$lib/canisters/ckdogecanister'
  import IconDOGE from '$lib/components/icons/IconDOGE.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import TextTokenAmount from '$lib/components/ui/TextTokenAmount.svelte'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { Chain } from '$lib/utils/dogecoin'
  import { DOGEToken } from '$lib/utils/token'
  import { isOnline, isVisible } from '$lib/utils/window'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import DogeReceiveModal from './DogeReceiveModal.svelte'
  import DogeSendModal from './DogeSendModal.svelte'

  let ckDogeCanisterAPI: CKDogeCanisterAPI
  let ckDogeCanisterState: Readable<State | null>
  let dogeBalance = Promise.resolve(0n)
  let chain = new Chain('test')
  let dogeAddress = '-/-'

  const modalStore = getModalStore()

  function receiveHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: DogeReceiveModal,
          props: {
            dogeAddress: dogeAddress,
            onFinish: () => {
              dogeBalance = ckDogeCanisterAPI.getBalance(dogeAddress)
            }
          }
        }
      })
    }
  }

  async function sendHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: DogeSendModal,
          props: {
            ckDogeCanisterAPI,
            sendFrom: dogeAddress,
            availableBalance: await dogeBalance,
            chain,
            onFinish: () => {
              dogeBalance = ckDogeCanisterAPI.getBalance(dogeAddress)
            }
          }
        }
      })
    }
  }

  onMount(() => {
    let interval = true
    ;(async () => {
      ckDogeCanisterAPI = await ckDogeCanisterAPIAsync()
      ckDogeCanisterState = ckDogeCanisterAPI.stateStore
      chain = new Chain($ckDogeCanisterState?.chain || 'test')
      if (!principal.isAnonymous()) {
        dogeAddress = await ckDogeCanisterAPI.getAddress()
        dogeBalance = ckDogeCanisterAPI.getBalance(dogeAddress)
      }
      const ms = principal.isAnonymous() ? 20000 : 10000

      while (interval) {
        await new Promise((res) => setTimeout(res, ms))
        if (isOnline() && isVisible()) {
          await ckDogeCanisterAPI?.refreshState()
        }
      }
    })()

    return () => {
      interval = false
    }
  })

  $: principal = $authStore.identity.getPrincipal()
  $: {
    if (ckDogeCanisterAPI) {
      ckDogeCanisterState = ckDogeCanisterAPI.stateStore
      if (ckDogeCanisterAPI?.principal.toString() != principal.toString()) {
        ckDogeCanisterAPIAsync().then(async (_api) => {
          ckDogeCanisterAPI = _api
          if (!principal.isAnonymous()) {
            dogeAddress = await ckDogeCanisterAPI.getAddress()
            dogeBalance = ckDogeCanisterAPI.getBalance(dogeAddress)
          }
        })
      }
    }
  }
</script>

<div class="card rounded-2xl bg-white">
  <header class="card-header flex flex-row items-center justify-between p-6">
    <div class="flex flex-row items-center gap-4">
      <span class="*:size-12"><IconDOGE /></span>
      <span>{DOGEToken.name}</span>
    </div>
    <div class="space-x-2 max-sm:hidden">
      <span>{dogeAddress}</span>
      <TextClipboardButton textValue={dogeAddress} />
    </div>
  </header>
  <section class="p-6 pt-0">
    <h3 class="h3 text-center font-extrabold">
      <TextTokenAmount class="" token={DOGEToken} amount={dogeBalance} />
    </h3>

    <p class="mt-2 min-w-0 text-balance break-words">
      DOGE is the native Dogecoin hold by the Internet Computer's <b
        >chain-key</b
      >
      for you. Check status on a
      <a
        class="font-bold text-secondary-500 underline underline-offset-4"
        href={chain.addressExplorer(dogeAddress)}
        target="_blank">block explorer</a
      >.
    </p>
  </section>
  <footer class="card-footer">
    <div class="flex flex-row items-center justify-center gap-4">
      <button
        on:click={receiveHandler}
        class="btn w-[200px] border-[1px] border-black font-medium text-black"
      >
        Receive
      </button>
      <button
        on:click={sendHandler}
        class="variant-filled btn w-[200px] bg-gray font-medium"
      >
        Send
      </button>
    </div>
    <p class="mt-4 min-w-0 text-balance break-words text-sm text-gray/50">
      Check tip
      <a
        class="font-bold text-secondary-500 underline underline-offset-4"
        href={chain.blockExplorer($ckDogeCanisterState?.tip_height || 0n)}
        target="_blank">{$ckDogeCanisterState?.tip_height || 0n}</a
      >: {$ckDogeCanisterState?.tip_blockhash || '--'}
    </p>
  </footer>
</div>
