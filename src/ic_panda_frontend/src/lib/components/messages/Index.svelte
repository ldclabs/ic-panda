<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { authStore } from '$lib/stores/auth'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import Chat from './Chat.svelte'
  import Home from './Home.svelte'
  import PasswordModel from './PasswordModel.svelte'

  const modalStore = getModalStore()

  let myState: MyMessageState
  let myInfo: Readable<UserInfo>

  async function initState() {
    myState = await myMessageStateAsync()
    myInfo = myState.info
    const mk = await myState.masterKey()
    if (!myState.principal.isAnonymous() && (!mk || !mk.isOpened())) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: PasswordModel,
          props: {
            myState: myState,
            myInfo: myInfo,
            masterKey: mk
          }
        }
      })
    }
  }

  onMount(() => {
    initState().catch((err) => console.error(err))
  })

  $: {
    if (myState) {
      if (
        myState.principal.toString() !=
        $authStore.identity.getPrincipal().toString()
      ) {
        initState().catch((err) => console.error(err))
      }
    }
  }
</script>

{#if $myInfo}
  <Chat {myInfo} />
{:else}
  <Home {myState} {myInfo} />
{/if}
