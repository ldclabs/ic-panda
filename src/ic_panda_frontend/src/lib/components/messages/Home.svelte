<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import Saos from 'saos'
  import { onMount, tick } from 'svelte'
  import { writable, type Writable } from 'svelte/store'

  export let myState: MyMessageState

  const toastStore = getToastStore()
  const latest_users: Writable<UserInfo[]> = writable([])

  let users_total = 0n
  let names_total = 0n
  let channels_total = 0n
  let messages_total = 0n

  onMount(() => {
    const { abort } = toastRun(async () => {
      users_total = myState.api.state?.users_total || 0n
      names_total = myState.api.state?.names_total || 0n
      await tick()

      const ids = myState.api.state?.channel_canisters || []
      const states = await Promise.all(
        ids.map(async (id) => {
          const api = myState.api.channelAPI(id)
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
        if (name.toLocaleLowerCase() !== 'panda') {
          const user = await myState.tryLoadUser(name)
          if (user) {
            latest_users.update((users) => [...users, user])
          }
        }
      }
    }, toastStore)
    return abort
  })
</script>

<div class="mt-12 flex max-w-4xl flex-1 flex-col px-4 md:mt-24">
  <Saos
    animation="scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both"
  >
    <div class="flex w-full flex-col items-center justify-center gap-10 px-4">
      <img
        class="w-[332px]"
        src="/_assets/icpanda-message.black.webp"
        alt="ICPanda message brand"
      />
      <p class="text-xl font-normal antialiased">
        ICPanda Message (dMsg.net) is a decentralized end-to-end encrypted
        messaging application fully running on the <a
          class="underline underline-offset-4"
          href="https://internetcomputer.org"
          target="_blank"
        >
          Internet Computer
        </a> blockchain.
      </p>
      <div class="w-full">
        <a
          type="button"
          class="rainbow-button bg-slate-950 group relative m-auto block w-64 overflow-hidden px-6 py-2 text-center text-white transition-all duration-300 ease-in-out hover:scale-105 active:scale-95"
          target="_blank"
          href="https://dmsg.net"
        >
          <span class="relative z-10 text-lg">Launch app (dMsg.net)</span>
          <span class="rainbow-border"></span>
        </a>
      </div>
    </div>
  </Saos>
  <Saos
    once={true}
    animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
  >
    <div
      class="mt-12 flex w-full max-w-5xl flex-col items-center justify-center gap-6 px-4 text-black md:mt-24"
    >
      <div class="text-center"><h2 class="h2">Key Features</h2></div>
      <div class="divide-neutral-500 flex flex-col gap-8 divide-y md:gap-10">
        <div>
          <h3 class="h3"
            ><span class="pr-2 text-5xl">🔐</span>End-to-end Encryption</h3
          >
          <p class="text-neutral-300 mt-4">
            All user messages are encrypted using the
            <a
              class="underline underline-offset-4"
              href="https://datatracker.ietf.org/doc/html/rfc9052"
              target="_blank">RFC 9052 (COSE)</a
            >
            standard and
            <b>quantum secure AES-256-GCM algorithm</b> on the client side and stored
            permanently on the ICP blockchain. These messages can only be decrypted
            on the client side.
          </p>
        </div>
        <div class="pt-8 md:pt-4">
          <h3 class="h3"
            ><span class="pr-2 text-5xl">💬</span>Multi-user Chats</h3
          >
          <p class="text-neutral-300 mt-4">
            Message channels support one-to-many chats, where a manager can add
            or remove members and exchange encryption keys. If the last manager
            leaves the channel, all messages in the channel are deleted.
          </p>
        </div>
        <div class="pt-8 md:pt-4">
          <h3 class="h3"><span class="pr-2 text-5xl">⛓</span>100% On-Chain</h3>
          <p class="text-neutral-300 mt-4">
            It runs entirely as a smart contract on the ICP blockchain,
            controlled by
            <a
              class="underline underline-offset-4"
              href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
              target="_blank">ICPanda DAO</a
            >, with fully open-source code. It is a trustworthy, secure,
            verifiable, and unstoppable Web3 application.
          </p>
        </div>
      </div>
    </div>
  </Saos>
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
    class="card mt-1 flex flex-col divide-y divide-gray/5 rounded-2xl rounded-t-none bg-transparent bg-white p-2 text-sm *:justify-start sm:p-8"
  >
    <a
      class="flex flex-row items-center space-x-2 rounded px-2 py-1 hover:variant-soft-primary"
      href="https://dmsg.net/PANDA"
    >
      <Avatar src="/_assets/logo.svg" fill="fill-white" width="w-10" />
      <span class="ml-1 truncate">ICPanda DAO</span>
      <span class="text-gray/60">@PANDA</span>
      <p class="truncate">Ask me anything</p>
    </a>
    {#each $latest_users as user (user.id.toText())}
      <a
        class="flex flex-row items-center space-x-2 rounded px-2 py-1 hover:variant-soft-primary"
        href="https://dmsg.net/{user.username[0]}"
      >
        <Avatar
          initials={user.name}
          src={user.image}
          fill="fill-white"
          width="w-10"
        />
        <span class="ml-1 truncate">{user.name}</span>
        <span class="text-gray/60">@{user.username[0]}</span>
      </a>
    {/each}
  </div>
</div>

<footer id="page-footer" class="flex-none">
  <PageFooter />
</footer>

<style>
  :global(.rainbow-button) {
    position: relative;
    border-radius: 9999px; /* 使用一个很大的值来确保完全圆角 */
  }

  :global(.rainbow-border) {
    position: absolute;
    inset: -3px;
    background-image: linear-gradient(
      to right,
      #00ffff,
      #1e90ff,
      #4b0082,
      #8a2be2,
      #00bfff,
      #1e90ff,
      #00ffff
    );
    background-size: 200% 100%;
    animation: move-gradient 4s linear infinite;
    z-index: 0;
    border-radius: 9999px;
    filter: blur(3px);
    opacity: 0.9;
    transition: all 0.3s ease;
  }

  :global(.rainbow-button:hover .rainbow-border) {
    filter: blur(2px);
    opacity: 1;
    inset: -6px;
  }

  :global(.rainbow-button::before) {
    content: '';
    position: absolute;
    inset: -1px;
    background: inherit;
    border-radius: inherit;
    filter: blur(3px);
    opacity: 0.6;
    z-index: -1;
  }

  :global(.rainbow-button::after) {
    content: '';
    position: absolute;
    inset: 4px;
    background: radial-gradient(
      circle,
      rgb(36, 44, 70) 60%,
      rgba(36, 44, 70, 0.9) 100%
    );
    border-radius: 9999px;
    z-index: 1;
    transition: all 0.3s ease;
  }

  :global(.rainbow-button:hover::after) {
    inset: 3px;
  }

  @keyframes move-gradient {
    0% {
      background-position: 0% 50%;
    }
    100% {
      background-position: 200% 50%;
    }
  }

  @keyframes -global-slide-top {
    0% {
      transform: translateY(100px);
      opacity: 0;
    }
    100% {
      transform: translateY(0);
      opacity: 1;
    }
  }

  @keyframes -global-scale-down-center {
    0% {
      transform: scale(1.1);
    }
    100% {
      transform: scale(1);
    }
  }
</style>
