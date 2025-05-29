<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/state'
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import { MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import type { Principal } from '@dfinity/principal'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, setContext } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelDetail from './ChannelDetail.svelte'
  import MyChannelList from './MyChannelList.svelte'
  import PasswordModal from './PasswordModal.svelte'

  interface Props {
    myState: MyMessageState
  }

  let { myState }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myInfo: Readable<UserInfo> =
    myState.agent.subscribeUser() as Readable<UserInfo>

  const channelParam = $derived((page?.params || {})['channel'] || '')
  const channelId = $derived(
    ChannelAPI.parseChannelParam(channelParam) as {
      canister: Principal
      id: number
    }
  )

  // svelte-ignore state_referenced_locally
  let isReady = $state(myState.isReady2())

  async function onChatBack() {
    goto('/_/messages')
  }

  setContext('onChatBack', onChatBack)

  onMount(() => {
    const { abort } = toastRun(async function () {
      if (!$myInfo) {
        return goto('/')
      }

      if (!isReady) {
        const iv = await myState.myIV()
        const mk = await myState.masterKey(iv)
        isReady = !!mk && mk.isOpened() && myState.masterKeyKind() === mk.kind
        if (!isReady) {
          modalStore.close()

          modalStore.trigger({
            type: 'component',
            component: {
              ref: PasswordModal,
              props: {
                myState: myState,
                masterKey: mk,
                onCompleted: () => {
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
    class="channels-list h-full w-full transition-transform duration-300 dark:bg-neutral-950 {channelId.canister
      ? 'max-sm:-translate-x-full'
      : ''} border-surface-500/20 bg-white max-sm:absolute max-sm:bottom-0 max-sm:top-0 max-sm:z-10 sm:border-r"
  >
    <MyChannelList {myState} />
  </div>
  <div class="h-full bg-surface-900/5">
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
