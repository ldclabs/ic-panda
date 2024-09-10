<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { authStore } from '$lib/stores/auth'
  import { toastRun } from '$lib/stores/toast'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, setContext } from 'svelte'
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
    myState = await myMessageStateAsync()
    myInfo = myState.info
    myInfo_ = myInfo as Readable<UserInfo>
    if ($myInfo) {
      const mk = await myState.masterKey()
      if (!mk || !mk.isOpened() || myState.masterKeyKind() !== mk.kind) {
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
  }

  setContext('InitState', initState)

  onMount(() => {
    const { abort } = toastRun(initState, toastStore)
    return abort
  })

  $: {
    if (myState) {
      if (
        $authStore.identity.getPrincipal().compareTo(myState.principal) != 'eq'
      ) {
        toastRun(initState, toastStore)
      }
    }
  }
</script>

{#if $myInfo}
  <Chat myInfo={myInfo_} />
{:else}
  <Home {myState} {myInfo} />
{/if}
