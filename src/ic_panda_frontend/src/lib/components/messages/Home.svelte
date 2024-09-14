<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import { signIn } from '$lib/services/auth'
  import { toastRun } from '$lib/stores/toast'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'
  import UserRegisterModel from './UserRegisterModel.svelte'

  export let myState: MyMessageState | null
  export let myInfo: Readable<UserInfo | null>

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const latest_users: Writable<UserInfo[]> = writable([])

  let users_total = 0n
  let names_total = 0n
  let channels_total = 0n
  let messages_total = 0n

  function getStartedHandler() {
    toastRun(async () => {
      if (!myState || myState.principal.isAnonymous()) {
        const res = await signIn({})
        myState = await myMessageStateAsync()
        myInfo = myState.info
        if (!$myInfo && res.success == 'ok') {
          modalStore.trigger({
            type: 'component',
            component: {
              ref: UserRegisterModel,
              props: {}
            }
          })
        }
      } else if (!$myInfo) {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModel,
            props: {}
          }
        })
      }
    }, toastStore)
  }

  onMount(() => {
    const { abort } = toastRun(async () => {
      if (!myState) {
        myState = await myMessageStateAsync()
      }

      users_total = myState.api.state?.users_total || 0n
      names_total = myState.api.state?.names_total || 0n
      await tick()

      const ids = myState.api.state?.channel_canisters || []
      const states = await Promise.all(
        ids.map(async (id) => {
          const api = await myState!.api.channelAPI(id)
          return await api.get_state()
        })
      )

      channels_total = states.reduce((acc, state) => {
        return acc + (state.channels_total || 0n)
      }, 0n)
      messages_total = states.reduce((acc, state) => {
        return acc + (state.messages_total || 0n)
      }, 0n)

      for (const name of myState.api.state?.latest_usernames || []) {
        const user = await myState.tryLoadUser(name)
        console.log('fetching user', name, user)
        if (user) {
          latest_users.update((users) => [...users, user])
        }
      }
    }, toastStore)
    return abort
  })
</script>

<div class="mt-12 max-w-4xl px-4">
  <div class="">
    <p class="text-lg font-normal antialiased">
      ICPanda Message is a decentralized end-to-end encrypted messaging
      application fully running on the <a
        class="font-bold text-secondary-500 underline underline-offset-4"
        href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
        target="_blank"
      >
        Internet Computer
      </a> blockchain. Key features:
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
        a manager can add or remove members and exchange encryption keys. If the
        last manager leaves the channel, all messages in the channel are deleted.</li
      >
      <li
        ><b>On-chain:</b> It operates entirely as a smart contract on the ICP
        blockchain, controlled by
        <a
          class="font-bold underline underline-offset-4"
          href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
          target="_blank">ICPanda DAO</a
        >, with fully open-source code. It is a trustworthy, secure, verifiable,
        and unstoppable Web3 application.</li
      >
    </ul>
  </div>
  <div
    class="mt-12 flex max-w-4xl flex-row items-center justify-center gap-6 max-sm:flex-col *:max-sm:w-60"
  >
    <button
      on:click={getStartedHandler}
      class="bg-slate-950 variant-filled btn w-[320px]"
    >
      <span class="text-center">Get started</span>
    </button>
  </div>
  <div
    class="card mt-8 flex flex-col justify-around gap-4 rounded-2xl rounded-b-none bg-transparent bg-white p-8 sm:mt-12 sm:flex-row"
  >
    <div class="flex flex-col items-center">
      <h3 class="h3 text-[28px] font-bold text-panda">
        <span class="text-sm font-normal text-gray">Users:</span>
        {users_total}
      </h3>
    </div>

    <div class="flex flex-col items-center">
      <h3 class="h3 text-[28px] font-bold text-panda">
        <span class="text-sm font-normal text-gray">Names:</span>
        {names_total}
      </h3>
    </div>
    <div class="flex flex-col items-center">
      <h3 class="h3 text-[28px] font-bold text-panda">
        <span class="text-sm font-normal text-gray">Channels:</span>
        {channels_total}
      </h3>
    </div>
    <div class="flex flex-col items-center">
      <h3 class="h3 text-[28px] font-bold text-panda">
        <span class="text-sm font-normal text-gray">Messages:</span>
        {messages_total}
      </h3>
    </div>
  </div>
  <div
    class="card mt-1 flex flex-col space-y-2 rounded-2xl rounded-t-none bg-transparent bg-white p-8 text-sm *:justify-start"
  >
    <div class="flex flex-row items-center space-x-2">
      <Avatar src="/_assets/logo.svg" fill="fill-white" width="w-10" />
      <span class="ml-1 truncate">ICPanda DAO</span>
      <a href="https://panda.fans/PANDA" class="text-gray/60 hover:text-panda">
        <span class="">@PANDA</span>
      </a>
      <p class="truncate">Ask me anything</p>
    </div>
    {#each $latest_users as user (user.id.toText())}
      <div class="flex flex-row items-center space-x-2">
        <Avatar initials={user.name} fill="fill-white" width="w-10" />
        <span class="ml-1 truncate">{user.name}</span>
        {#if user.username.length > 0}
          <a
            href="https://panda.fans/{user.username[0]}"
            class="text-gray/60 hover:text-panda"
          >
            <span class="">@{user.username[0]}</span>
          </a>
        {/if}
      </div>
    {/each}
  </div>
</div>

<footer id="page-footer" class="flex-none">
  <PageFooter />
</footer>
