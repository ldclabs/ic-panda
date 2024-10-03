<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { toastRun } from '$lib/stores/toast'
  import { type UserInfo } from '$lib/canisters/message'
  import { MyMessageState } from '$lib/stores/message'
  import { sleep } from '$lib/utils/helper'
  import { setContext } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelDetail from './ChannelDetail.svelte'
  import MyChannelList from './MyChannelList.svelte'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import PasswordModel from './PasswordModel.svelte'
  import { onMount } from 'svelte'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import type { Principal } from '@dfinity/principal'

  export let myState: MyMessageState

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myInfo: Readable<UserInfo> =
    myState.agent.subscribeUser() as Readable<UserInfo>

  $: channelParam = ($page?.params || {})['channel'] || ''
  $: channelId = ChannelAPI.parseChannelParam(channelParam) as {
    canister: Principal
    id: number
  }
  $: channelsListActive = !channelId.canister
  $: isReady = myState.isReady2()

  async function onChatBack() {
    channelsListActive = true
    await sleep(300)
    goto('/_/messages')
  }

  setContext('onChatBack', onChatBack)

  onMount(() => {
    const { abort } = toastRun(async function () {
      if (myState.principal.isAnonymous() || !$myInfo) {
        return goto('/')
      }

      if (!isReady) {
        const iv = await myState.myIV()
        const mk = await myState.masterKey(iv)
        isReady = !!mk && mk.isOpened() && myState.masterKeyKind() === mk.kind
        if (!isReady) {
          modalStore.close()
          await sleep(618)

          modalStore.trigger({
            type: 'component',
            component: {
              ref: PasswordModel,
              props: {
                myState: myState,
                masterKey: mk,
                onFinished: () => {
                  isReady = myState.isReady2()
                }
              }
            }
          })
        }
      }
    }, toastStore)
    return abort
  })
</script>

<div
  class="relative h-full w-full sm:grid sm:grid-cols-[220px_1fr] md:grid-cols-[280px_1fr] lg:grid-cols-[320px_1fr]"
>
  <div
    class="channels-list h-full transition-transform duration-300 {channelsListActive
      ? ''
      : 'max-sm:-translate-x-full'} border-surface-500/20 bg-white sm:border-r"
  >
    <MyChannelList {myState} />
  </div>
  <div class="h-full bg-gray-900/5">
    {#key channelParam}
      {#if channelId.canister && isReady}
        <ChannelDetail {channelId} {myState} {myInfo} />
      {:else}
        <div class="flex h-full flex-col items-center justify-center">
          <img
            class="w-24"
            src="/_assets/logo.svg"
            alt="ICPanda message logo"
          />
        </div>
      {/if}
    {/key}
  </div>
</div>

<style>
  @media (max-width: 640px) {
    .channels-list {
      position: absolute;
      top: 0;
      bottom: 0;
      z-index: 10;
      width: 100%;
    }
  }
</style>
