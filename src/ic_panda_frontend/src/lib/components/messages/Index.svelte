<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { sleep } from '$lib/utils/helper'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import Chat from './Chat.svelte'
  import Home from './Home.svelte'
  import PasswordModel from './PasswordModel.svelte'

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let myState: MyMessageState
  let myInfo: Readable<UserInfo | null>
  let myInfo_: Readable<UserInfo>

  async function initState() {
    myState = await MyMessageState.load()
    if (myState.principal.isAnonymous()) {
      return
    }

    myInfo = myState.agent.subscribeUser()
    if (myState.isReady2()) {
      if (!myInfo_) {
        myInfo_ = myInfo as Readable<UserInfo>
      }
      return
    }
    const iv = await myState.myIV()
    const mk = await myState.masterKey(iv)
    myInfo_ = myInfo as Readable<UserInfo>

    if (!mk || !mk.isOpened() || myState.masterKeyKind() !== mk.kind) {
      modalStore.close()
      await sleep(618)

      modalStore.trigger({
        type: 'component',
        component: {
          ref: PasswordModel,
          props: {
            myState: myState,
            masterKey: mk
          }
        }
      })
    }
  }

  onMount(() => {
    const { abort } = toastRun(initState, toastStore)
    return abort
  })
</script>

{#if myInfo_}
  <Chat {myState} myInfo={myInfo_} />
{:else if myState}
  <Home {myState} />
{/if}
