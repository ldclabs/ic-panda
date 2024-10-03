<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { APP_ORIGIN } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
  import { toastRun } from '$lib/stores/toast'
  import { MyMessageState } from '$lib/stores/message'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, tick } from 'svelte'
  import { writable, type Writable } from 'svelte/store'
  import UserRegisterModel from '$lib/components/messages/UserRegisterModel.svelte'
  import Saos from 'saos'

  interface Saying {
    name: string
    image: string
    handle: string
    message: string
    messageUrl: string
  }

  interface Partner {
    title: string
    image: string
    url: string
    bg?: string
  }

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const latest_users: Writable<UserInfo[]> = writable([])
  const partners: Partner[] = [
    {
      title: 'The Internet Computer',
      image: '/_assets/images/internet-computer.webp',
      url: 'https://internetcomputer.org'
    },
    {
      title: 'ICPSwap',
      image: '/_assets/images/icpswap.webp',
      url: 'https://www.icpswap.com/',
      bg: 'bg-gray-900'
    },
    {
      title: 'OpenChat',
      image: '/_assets/images/openchat.webp',
      url: 'https://oc.app/home'
    },
    {
      title: 'ICPCoins',
      image: '/_assets/images/icpcoins.webp',
      url: 'https://icpcoins.com'
    }
  ]

  const saying_list: Saying[] = [
    {
      name: 'Ajki',
      image:
        'https://pbs.twimg.com/profile_images/1516102462347915278/agqi9CB2_200x200.jpg',
      handle: 'ajki76',
      message: `#InternetComputer $ICP breakthrough in E2EE (End-to-End Encryption) Messaging!💡 #ICPanda revolutionizes secure communication with blockchain-powered encryption. 🔐 ...`,
      messageUrl: 'https://x.com/ajki76/status/1836100231622201785'
    },
    {
      name: 'jan.icp ∞',
      image:
        'https://pbs.twimg.com/profile_images/1811395883612721152/szXkkW-7_200x200.jpg',
      handle: 'JanCamenisch',
      message: `fully e2e encrypted messaging fully e2e decentralized: talk to me at http://panda.fans/jan and experience the power of #icp @ICPandaDAO`,
      messageUrl: 'https://x.com/JanCamenisch/status/1838317278351282431'
    },
    {
      name: 'jason.icp ∞',
      image:
        'https://pbs.twimg.com/profile_images/1788932421834113024/57RoZo_C_200x200.jpg',
      handle: 'ICP_Japan',
      message: `All hacked roads lead to $ICP and @UtopiaCloudLabs by @dfinity ✨♾✨ Did you know that @ICPandaDAO recently launched a fully decentralised e2e encrypted messaging service that runs on the $ICP blockchain? ⛓️‍💥🐼💬🐼⛓️‍💥`,
      messageUrl: 'https://x.com/ICP_Japan/status/1837003852220092738'
    },
    {
      name: 'DFINITY Developers ∞',
      image:
        'https://pbs.twimg.com/profile_images/1700228415327023104/l4MdrvwA_200x200.jpg',
      handle: 'DFINITYDev',
      message: `@ICPandaDAO Messaging (https://panda.fans/_/messages) is a decentralized end-to-end encrypted #messaging app developed on #ICP. It uses CBOR Object Signing and Encryption standard by the @ietf within ICP canisters. Learn more 👇`,
      messageUrl: 'https://x.com/DFINITYDev/status/1838335826184540319'
    },
    {
      name: 'goodcoin.icp ∞',
      image:
        'https://pbs.twimg.com/profile_images/1653076547107012615/JUgSoeNy_200x200.jpg',
      handle: 'plsak',
      message: `[#ICP/#ecosystem] 👉 I originally thought that $PANDA (@ICPandaDAO) is just a meme, but they do #AI, they do #messaging, I dunno, looks #coolish 🤞`,
      messageUrl: 'https://x.com/plsak/status/1836076387477704789'
    },
    {
      name: 'CaptainOblivious, G.e.D🐉ICP-XTC-BOB, mostly ICP',
      image:
        'https://pbs.twimg.com/profile_images/1805268628692226048/w2MXQEvm_200x200.jpg',
      handle: 'idontpfreely',
      message: `When ICpanda first came around, I admit, i shit on em pretty bad. They've proven to be a highly capable team on the IC. My bad.`,
      messageUrl: 'https://x.com/idontpfreely/status/1838805032596025568'
    },
    {
      name: 'sai ∞',
      image:
        'https://pbs.twimg.com/profile_images/1837659383674925056/kWU5f-px_200x200.jpg',
      handle: 'yoopsbro',
      message: `All this talk about Quantum computers being the next existential threat to our IT and then this team has already created a solution lol.`,
      messageUrl: 'https://x.com/yoopsbro/status/1840375057467756986'
    }
  ]

  let myState: MyMessageState
  let users_total = 0n
  let names_total = 0n
  let channels_total = 0n
  let messages_total = 0n

  async function onLaunchAppHandler() {
    // always load the latest one
    myState = await MyMessageState.load()
    if (myState.principal.isAnonymous()) {
      await authStore.signIn({})
      myState = await MyMessageState.load()
      onLaunchAppHandler()
    } else if (!myState.api.myInfo) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModel,
          props: {
            myState,
            onFinished: onLaunchAppHandler
          }
        }
      })
    } else {
      goto('/_/messages')
    }
  }

  onMount(() => {
    const { abort } = toastRun(async () => {
      myState = await MyMessageState.load()
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

      for (const name of myState.api.state?.latest_usernames.slice(0, 7) ||
        []) {
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

<div class="landing-page w-full">
  <div
    class="pandas-backgroud flex w-full max-w-[1800px] flex-col items-center pt-12 md:pt-24"
  >
    <Saos
      animation={'scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both'}
    >
      <div
        class="flex w-full max-w-3xl flex-col items-center justify-center gap-10 px-4"
      >
        <img
          class="w-[332px]"
          src="/_assets/icpanda-message.webp"
          alt="ICPanda message brand"
        />
        <p class="text-xl font-normal text-white antialiased">
          ICPanda Message (dMsg.net) is a decentralized end-to-end encrypted
          messaging application fully running on the <a
            class="underline underline-offset-4"
            href="https://internetcomputer.org"
            target="_blank"
          >
            Internet Computer
          </a> blockchain.
        </p>
        <button
          on:click={onLaunchAppHandler}
          class="btn w-[320px] bg-white shadow-2xl transition-transform duration-300 ease-in-out hover:scale-105 active:scale-95"
        >
          <span class="text-lg">Launch app</span>
        </button>
      </div>
    </Saos>
    <Saos
      once={true}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <div
        class="mt-12 flex w-full max-w-4xl flex-col items-center justify-center gap-6 px-4 text-white md:mt-24"
      >
        <div class="text-center"><h2 class="h2">Key Features</h2></div>
        <div class="flex flex-col gap-8 divide-y divide-neutral-500 md:gap-10">
          <div>
            <h3 class="h3"
              ><span class="pr-2 text-5xl">🔐</span>End-to-end Encryption</h3
            >
            <p class="mt-4 text-neutral-300">
              All user messages are encrypted using the
              <a
                class="text-white underline underline-offset-4"
                href="https://datatracker.ietf.org/doc/html/rfc9052"
                target="_blank">RFC 9052 (COSE)</a
              >
              standard and
              <b class="text-white">quantum secure AES-256-GCM algorithm</b> on the
              client side and stored on the ICP blockchain. These messages can only
              be decrypted on the client side.
            </p>
          </div>
          <div class="pt-8 md:pt-10">
            <h3 class="h3"
              ><span class="pr-2 text-5xl">💬</span>Multi-user Chats</h3
            >
            <p class="mt-4 text-neutral-300">
              Message channels support one-to-many chats, where a manager can
              add or remove members and exchange encryption keys. If the last
              manager leaves the channel, all messages in the channel are
              deleted.
            </p>
          </div>
          <div class="pt-8 md:pt-10">
            <h3 class="h3"
              ><span class="pr-2 text-5xl">⛓</span>100% On-Chain</h3
            >
            <p class="mt-4 text-neutral-300">
              It operates entirely as a smart contract on the ICP blockchain,
              controlled by
              <a
                class="text-white underline underline-offset-4"
                href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
                target="_blank">ICPanda DAO</a
              >, with fully open-source code. It is a trustworthy, secure,
              verifiable, and unstoppable Web3 application.
            </p>
          </div>
        </div>
      </div>
    </Saos>
    <Saos
      once={true}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <div
        class="mt-12 flex w-full max-w-4xl flex-col items-center justify-center gap-6 px-4 text-white md:mt-24"
      >
        <div class="text-center">
          <h2 class="h2">Service States</h2>
        </div>
        <div
          class="mt-2 flex flex-row flex-wrap items-center justify-center gap-6 *:flex *:h-28 *:w-40 *:flex-col *:justify-center *:rounded-2xl *:border *:border-neutral-400 *:text-center lg:*:w-48"
        >
          <div class="transition duration-300 hover:scale-105">
            <h3 class="h3 text-panda md:text-3xl">
              {users_total}
            </h3>
            <p>Total Users</p>
          </div>
          <div class="transition duration-300 hover:scale-105">
            <h3 class="h3 text-panda md:text-3xl">
              {names_total}
            </h3>
            <p>Usernames</p>
          </div>
          <div class="transition duration-300 hover:scale-105">
            <h3 class="h3 text-panda md:text-3xl">
              {channels_total}
            </h3>
            <p>Channels</p>
          </div>
          <div class="transition duration-300 hover:scale-105">
            <h3 class="h3 text-panda md:text-3xl">
              {messages_total}
            </h3>
            <p>Messages</p>
          </div>
        </div>
      </div>
      <div class="mt-10 w-full max-w-4xl px-4 text-white">
        <div
          class="mt-6 flex w-full flex-col items-center justify-center gap-1"
        >
          <a
            class="group grid w-full grid-cols-[1fr_auto] items-center rounded p-2 text-neutral-400 hover:variant-soft hover:text-white"
            href="{APP_ORIGIN}/PANDA"
          >
            <div class="flex flex-row items-center space-x-2 max-md:max-w-72">
              <Avatar src="/_assets/logo.svg" fill="fill-white" width="w-10" />
              <span class="ml-1 truncate text-neutral-200">ICPanda</span>
              <span class="">@PANDA</span>
              <p class="truncate max-md:hidden">| Ask me anything</p>
            </div>
          </a>
          {#each $latest_users as user (user.id.toText())}
            <a
              class="group grid w-full grid-cols-[1fr_auto] items-center rounded p-2 text-neutral-400 hover:variant-soft hover:text-white"
              href="{APP_ORIGIN}/{user.username[0]}"
            >
              <div class="flex flex-row items-center space-x-2">
                <Avatar
                  initials={user.name}
                  src={user.image}
                  fill="fill-white"
                  width="w-10"
                />
                <span class="ml-1 truncate text-neutral-200">{user.name}</span>
                <span class="text-neutral-600">@{user.username[0]}</span>
              </div>
            </a>
          {/each}
        </div>
      </div>
    </Saos>

    <div
      class="mt-12 flex w-full flex-col items-center justify-center gap-6 px-4 text-white md:mt-24"
    >
      <div class="text-center">
        <h2 class="h2">Ecosystem partners</h2>
      </div>
      <div
        class="mt-2 flex w-full snap-x snap-mandatory scroll-px-6 gap-6 overflow-x-auto overscroll-x-contain scroll-smooth px-6 pb-6 md:justify-center"
      >
        {#each partners as partner (partner.url)}
          <a
            class="shrink-0 snap-center rounded-sm p-2 {partner.bg ||
              'bg-white'}"
            target="_blank"
            href={partner.url}
          >
            <img class="h-10" src={partner.image} alt={partner.title} />
          </a>
        {/each}
      </div>
    </div>
    <div
      class="mt-12 flex w-full flex-col items-center justify-center gap-6 px-4 text-white md:mt-24"
    >
      <div class="text-center">
        <h2 class="h2">What people are saying</h2>
      </div>
      <div
        class="mt-2 flex w-full snap-x snap-mandatory scroll-px-6 gap-6 overflow-x-auto overscroll-x-contain scroll-smooth px-6 pb-6 *:w-80 *:rounded-xl *:border *:border-neutral-400"
      >
        {#each saying_list as saying (saying.messageUrl)}
          <a
            class="shrink-0 snap-center p-6 hover:variant-soft hover:text-white"
            target="_blank"
            href={saying.messageUrl}
          >
            <div class="flex flex-row items-center space-x-2">
              <Avatar src={saying.image} width="w-10" />
              <span class="truncate">{saying.name}</span>
              <span class="text-neutral-400">@{saying.handle}</span>
            </div>
            <p class="mt-2 text-balance text-neutral-300">{saying.message}</p>
          </a>
        {/each}
      </div>
    </div>

    <footer id="page-footer" class="px-4 pb-24 pt-12">
      <div class="flex h-16 flex-col items-center">
        <p class="flex flex-row items-center gap-1 text-white">
          <span class="text-sm">© 2024</span>
          <a class="" href="https://panda.fans" target="_blank"
            ><img
              class="w-28"
              src="/_assets/icpanda-dao-white.svg"
              alt="ICPanda DAO"
            /></a
          >
        </p>
        <p
          class="mt-2 text-center text-sm capitalize text-neutral-300 antialiased"
        >
          A decentralized Panda meme brand fully running on the <a
            class="underline underline-offset-4"
            href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
            target="_blank"
          >
            Internet Computer
          </a> blockchain.
        </p>
      </div>
    </footer>
  </div>
</div>

<style>
  .landing-page {
    background-color: hsla(0, 100%, 1%, 1);
    background-image: radial-gradient(
        at 0% 0%,
        hsla(137, 58%, 16%, 0.98) 0px,
        transparent 50%
      ),
      radial-gradient(at 40% 20%, hsla(27, 100%, 1%, 1) 0px, transparent 50%),
      radial-gradient(
        at 100% 60%,
        hsla(137, 58%, 16%, 0.98) 0px,
        transparent 50%
      );
  }

  .pandas-backgroud {
    background-image: url('/_assets/images/pandas.webp');
    background-repeat: repeat-x;
    background-size: auto 320px;
    animation: slideBackground 60s linear infinite;

    @media (min-width: 640px) {
      background-size: auto 500px;
    }
  }

  @keyframes slideBackground {
    from {
      background-position: 0 0;
    }
    to {
      background-position: 1734px 0;
    }
  }

  /**
  * ----------------------------------------
  * animation slide-top http://animista.net
  * ----------------------------------------
  */
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
