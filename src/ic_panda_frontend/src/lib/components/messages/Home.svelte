<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import { signIn } from '$lib/services/auth'
  import { toastRun } from '$lib/stores/toast'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import UserRegisterModel from './UserRegisterModel.svelte'

  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>

  const modalStore = getModalStore()

  function getStartedHandler() {
    toastRun(async () => {
      if (myState.principal.isAnonymous()) {
        const res = await signIn({})
        myState = await myMessageStateAsync()
        myInfo = myState.info
        if (!$myInfo && res.success == 'ok') {
          modalStore.trigger({
            type: 'component',
            component: { ref: UserRegisterModel }
          })
        }
      } else if (!$myInfo) {
        modalStore.trigger({
          type: 'component',
          component: { ref: UserRegisterModel }
        })
      }
    })
  }

  onMount(() => {
    if ($page.params['channel']) {
      goto('/_/messages')
    }
  })
</script>

<div class="mt-4 max-w-4xl">
  <p class="text-lg font-normal antialiased">
    ICPanda Message is a decentralized end-to-end encrypted messaging
    application fully running on the <a
      class="font-bold text-secondary-500 underline underline-offset-4"
      href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
      target="_blank"
    >
      Internet Computer
    </a> blockchain. Features:
  </p>
  <ul class="mt-4 flex flex-col gap-4 pl-8">
    <li
      ><b>End-to-end encryption:</b> All user messages are encrypted using the
      <a
        class="font-bold underline underline-offset-4"
        href="https://datatracker.ietf.org/doc/html/rfc9052"
        target="_blank">RFC 9052 (COSE)</a
      > standard on the client side and stored on the ICP blockchain. These messages
      can only be decrypted on the client side.</li
    >
    <li
      ><b>Multi-user chats:</b> Message channels support one-to-many chats, where
      a manager can add or remove members and exchange encryption keys. If the last
      manager leaves the channel, all messages in the channel are deleted.</li
    >
  </ul>
</div>
<div
  class="mt-8 flex max-w-4xl flex-row items-center justify-center gap-6 max-sm:flex-col *:max-sm:w-60"
>
  <button
    on:click={getStartedHandler}
    class="bg-slate-950 variant-filled btn w-[320px]"
  >
    <span class="text-center">Get started</span>
  </button>
</div>
