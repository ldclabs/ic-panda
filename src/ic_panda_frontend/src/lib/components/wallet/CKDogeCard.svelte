<script lang="ts">
  import {
    CKDogeCanisterAPI,
    ckDogeCanisterAPIAsync,
    type State
  } from '$lib/canisters/ckdogecanister'
  import {
    CKDogeMinterAPI,
    ckDogeMinterAPIAsync
  } from '$lib/canisters/ckdogeminter'
  import { ckDOGETokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconCkDOGE from '$lib/components/icons/IconCkDOGE.svelte'
  import IconDOGE from '$lib/components/icons/IconDOGE.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import TextTokenAmount from '$lib/components/ui/TextTokenAmount.svelte'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { errMessage } from '$lib/types/result'
  import { Chain, toHashString } from '$lib/utils/dogecoin'
  import { shortId } from '$lib/utils/helper'
  import { ckDOGEToken, DOGEToken, formatNumber } from '$lib/utils/token'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import CKDogeReceiveModal from './CKDogeReceiveModal.svelte'
  import CKDogeSendModal from './CKDogeSendModal.svelte'

  interface UtxoInfo {
    txid: string
    vout: number
    value: bigint
    height: bigint
    minted_at: Date | null
  }

  let ckDogeMinterAPI: CKDogeMinterAPI
  let ckDogeCanisterAPI: CKDogeCanisterAPI
  let ckDogeCanisterState: Readable<State | null>
  let ckDogeBalance = Promise.resolve(0n)
  let chain = new Chain('test')
  let ckDogeAddress = '-/-'
  let utxos: UtxoInfo[] = []
  let mintable = 0n
  let min_confirmations = 42

  const modalStore = getModalStore()
  const toastStore = getToastStore()

  function receiveHandler() {
    if (principal.isAnonymous()) {
      signIn()
    } else {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: CKDogeReceiveModal,
          props: {
            dogeAddress: ckDogeAddress,
            principal: principal.toString(),
            onFinish: () => {
              ckDogeBalance = ckDOGETokenLedgerAPI.balance()
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
          ref: CKDogeSendModal,
          props: {
            ckDogeMinterAPI,
            tokenLedgerAPI: ckDOGETokenLedgerAPI,
            principal,
            dogeAddress: ckDogeAddress,
            availableBalance: await ckDogeBalance,
            chain,
            onFinish: () => {
              ckDogeBalance = ckDOGETokenLedgerAPI.balance()
            }
          }
        }
      })
    }
  }

  let refreshUTXOs = true
  async function fetchUTXOs(): Promise<UtxoInfo[]> {
    const res = []
    if (!principal.isAnonymous()) {
      refreshUTXOs = false
      const mintedUTXOs = await ckDogeMinterAPI.list_minted_utxos()
      mintedUTXOs.reverse()
      const minted = new Set<string>()
      for (const val of mintedUTXOs) {
        const txid = toHashString(val.utxo.txid)
        minted.add(txid + val.utxo.vout)
        res.push({
          txid,
          vout: val.utxo.vout,
          value: val.utxo.value,
          height: val.utxo.height,
          minted_at: new Date(Number(val.minted_at))
        })
      }

      const utxosOutput = await ckDogeCanisterAPI.listUtxos(ckDogeAddress)
      for (const val of utxosOutput.utxos) {
        const txid = toHashString(val.txid)
        if (minted.has(txid + val.vout)) {
          continue
        }

        res.unshift({
          txid,
          vout: val.vout,
          value: val.value,
          height: val.height,
          minted_at: null
        })
      }
    }

    setTimeout(() => {
      refreshUTXOs = true
    }, 10000)
    return res
  }

  let isLoadingUTXOs = false
  async function fetchUTXOsHandler() {
    isLoadingUTXOs = true
    try {
      const [res, _] = await Promise.all([
        fetchUTXOs(),
        new Promise((resolve) => setTimeout(resolve, 2000))
      ])

      utxos = res
      mintable = utxos.reduce(
        (acc, val) =>
          val.minted_at == null &&
          val.height + BigInt(min_confirmations) <= tipHeight
            ? acc + val.value
            : acc,
        0n
      )
    } catch (err: any) {
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err).replaceAll('\\n', '<br />')
      })
    } finally {
      isLoadingUTXOs = false
    }
  }

  let isMinting = false
  async function mintCkDOGEHandler() {
    if (mintable == 0n) {
      return
    }

    isMinting = true
    try {
      const mintOutput = await ckDogeMinterAPI.mintCKDoge()
      ckDogeBalance = ckDOGETokenLedgerAPI.balance()
      fetchUTXOsHandler()
      mintable == 0n
      toastStore.trigger({
        background: 'variant-filled-success',
        message: `Minted ${formatNumber(
          Number(mintOutput.amount) / Number(DOGEToken.one),
          8
        )} ckDOGE`
      })
    } catch (err: any) {
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err).replaceAll('\\n', '<br />')
      })
    } finally {
      isMinting = false
    }
  }

  onMount(async () => {
    ckDogeMinterAPI = await ckDogeMinterAPIAsync()
    ckDogeCanisterAPI = await ckDogeCanisterAPIAsync()

    ckDogeCanisterState = ckDogeCanisterAPI.stateStore
    chain = new Chain($ckDogeCanisterState?.chain || 'test')
    min_confirmations =
      $ckDogeCanisterState?.min_confirmations || min_confirmations
    if (!principal.isAnonymous()) {
      ckDogeBalance = ckDOGETokenLedgerAPI.balance()
      ckDogeAddress = await ckDogeMinterAPI.getAddress()
    }
  })

  $: principal = $authStore.identity.getPrincipal()
  $: tipHeight = $ckDogeCanisterState?.tip_height || 0n
  $: {
    if (ckDOGETokenLedgerAPI) {
      ckDogeCanisterState = ckDogeCanisterAPI.stateStore
      ckDogeMinterAPIAsync().then(async (_api) => {
        ckDogeMinterAPI = _api
        if (!principal.isAnonymous()) {
          ckDogeAddress = await ckDogeMinterAPI.getAddress()
        }
      })
    }
  }
</script>

<div class="card rounded-2xl bg-white">
  <header class="card-header flex flex-row items-center justify-between p-6">
    <div class="flex flex-row items-center gap-4">
      <span class="*:size-12"><IconCkDOGE /></span>
      <span>{ckDOGEToken.name}</span>
    </div>
    <div class="space-x-2 max-sm:hidden">
      <span>{shortId(principal.toString(), true)}</span>
      <TextClipboardButton textValue={principal.toString()} />
    </div>
  </header>
  <section class="p-6 pt-0">
    <h3 class="h3 text-center font-extrabold">
      <TextTokenAmount class="" token={ckDOGEToken} amount={ckDogeBalance} />
    </h3>

    <p class="mt-2 min-w-0 text-balance break-words">
      ckDOGE is a multi-chain Dogecoin twin on the Internet Computer created by
      canister smart contracts that directly hold native Dogecoin. Note:
      incoming Dogecoin transactions require <span class="font-bold"
        >{min_confirmations}</span
      >
      confirmations. Check status on a
      <a
        class="font-bold text-secondary-500 underline underline-offset-4"
        href={chain.addressExplorer(ckDogeAddress)}
        target="_blank">block explorer</a
      >.
    </p>
    <div
      class="card mt-4 flex flex-row items-center justify-between bg-gray/5 p-4"
    >
      <div class="flex flex-row items-center gap-4">
        <span class="*:size-12"><IconDOGE /></span>
        <div class="flex flex-col">
          <p class="font-bold">DOGE address to mint ckDOGE:</p>
          <p class="min-w-0 text-balance break-words max-sm:max-w-52"
            >{ckDogeAddress}</p
          >
        </div>
      </div>
      <div class="space-x-2">
        <TextClipboardButton textValue={ckDogeAddress} />
      </div>
    </div>
    {#if utxos.length > 0}
      <div class="utxos table-container mt-6">
        <table class="table table-hover bg-white">
          <thead>
            <tr>
              <th>Transaction</th>
              <th>Amount</th>
              <th>Confirmations</th>
              <th>Minted</th>
            </tr>
          </thead>
          <tbody>
            {#each utxos as row}
              <tr>
                <td
                  ><a
                    class="block w-60 truncate underline underline-offset-4"
                    href={chain.txExplorer(row.txid)}
                    target="_blank">{row.txid}</a
                  ></td
                >
                <td
                  >{formatNumber(
                    Number(row.value) / Number(DOGEToken.one),
                    8
                  )}</td
                >
                <td>{tipHeight - row.height}</td>
                <td
                  >{row.minted_at?.toLocaleString() ||
                    (tipHeight - row.height >= min_confirmations
                      ? 'mintable'
                      : 'wait confirmations')}</td
                >
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    <div class="mt-4">
      <button
        disabled={isLoadingUTXOs || !refreshUTXOs}
        on:click={fetchUTXOsHandler}
        class="btn rounded-md p-0 {!isLoadingUTXOs && refreshUTXOs
          ? 'underline underline-offset-4'
          : ''}"
      >
        {#if isLoadingUTXOs}
          <span>Check for incoming DOGE:</span>
          <span class="*:size-5"><IconCircleSpin /></span>
          <span>Checking...</span>
        {:else if mintable > 0n}
          <span>
            Mintable DOGE: {formatNumber(
              Number(mintable) / Number(DOGEToken.one),
              8
            )}
          </span>
        {:else}
          <span>Check for incoming DOGE</span>
        {/if}
      </button>
      {#if mintable > 0n}
        <button
          on:click={mintCkDOGEHandler}
          class="variant-filled btn btn-sm ml-2"
        >
          {#if isMinting}
            <span class="*:size-4"><IconCircleSpin /></span>
            <span>Minting...</span>
          {:else}
            <span>Mint ckDOGE</span>
          {/if}
        </button>
      {/if}
    </div>
  </section>
  <footer class="card-footer my-4">
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
  </footer>
</div>

<style>
  .utxos thead th {
    padding: 8px;
  }
  .utxos tbody td {
    padding: 8px;
  }
</style>
