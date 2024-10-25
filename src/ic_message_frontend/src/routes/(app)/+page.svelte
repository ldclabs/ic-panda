<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import MoreMenuPopup from '$lib/components/core/MoreMenuPopup.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
  import { MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { agent } from '$lib/utils/auth'
  import { initPopup } from '$lib/utils/popup'
  import UserRegisterModal from '$src/lib/components/messages/UserRegisterModal.svelte'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import Saos from 'saos'
  import { onDestroy, onMount, tick } from 'svelte'
  import { writable, type Writable } from 'svelte/store'

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
      url: 'https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai'
    },
    {
      title: 'ICPSwap',
      image: '/_assets/images/icpswap.webp',
      url: 'https://info.icpswap.com/swap/pool/details/5fq4w-lyaaa-aaaag-qjqta-cai'
    },
    {
      title: 'CoinGecko',
      image: '/_assets/images/coingecko.webp',
      url: 'https://www.coingecko.com/en/coins/icpanda-dao'
    },
    {
      title: 'DefiLlama',
      image: '/_assets/images/defillama.webp',
      url: 'https://defillama.com/governance/icpanda-dao'
    },
    {
      title: 'OpenChat',
      image: '/_assets/images/openchat.webp',
      url: 'https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai'
    },
    {
      title: 'ICPTokens',
      image: '/_assets/images/icptokens.webp',
      url: 'https://icptokens.net/token/druyg-tyaaa-aaaaq-aactq-cai'
    },
    {
      title: 'ICPCoins',
      image: '/_assets/images/icpcoins.webp',
      url: 'https://icpcoins.com/#/token/PANDA'
    },
    {
      title: 'ICLight',
      image: '/_assets/images/iclight.webp',
      url: 'https://iclight.io/ICDex/PANDA/ICP'
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

  const { popupOpenOn, popupDestroy } = initPopup({
    target: 'popupNavigationMore'
  })

  $: isAnonymous = agent.isAnonymous()

  let myState: MyMessageState
  let users_total = 0n
  let names_total = 0n
  let channels_total = 0n
  let messages_total = 0n

  async function onLaunchAppHandler() {
    // always load the latest one
    isAnonymous = agent.isAnonymous()
    myState = await MyMessageState.load()
    if (isAnonymous) {
      await authStore.signIn({})
      myState = await MyMessageState.load()
      onLaunchAppHandler()
    } else if (!myState.api.myInfo) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModal,
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

  onDestroy(() => {
    popupDestroy()
  })
</script>

<div class="landing-page w-full">
  <div
    class="pandas-backgroud flex w-full max-w-[1800px] flex-col items-center pt-12 md:pt-24"
  >
    <Saos
      animation="scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both"
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
        <div class="flex flex-row gap-2">
          <button
            on:click={onLaunchAppHandler}
            class="rainbow-button group relative w-[280px] overflow-hidden border border-black/50 bg-white px-6 py-2 shadow-2xl transition-all duration-300 ease-in-out hover:scale-105 active:scale-95"
          >
            <span
              class="relative z-10 text-lg text-neutral-700 transition-all duration-300 group-hover:text-neutral-950"
              >Launch app</span
            >
            <span class="rainbow-border"></span>
          </button>
          {#if !isAnonymous}
            <button
              class="btn px-4 text-white transition-all hover:scale-125"
              on:click={(ev) => {
                popupOpenOn(ev.currentTarget)
              }}
            >
              <span><IconMoreFill /></span>
            </button>
          {/if}
        </div>
      </div>
    </Saos>
    <Saos
      once={true}
      animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
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
              client side and stored permanently on the ICP blockchain. These messages
              can only be decrypted on the client side.
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
              It runs entirely as a smart contract on the ICP blockchain,
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
      animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
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
                <span class="text-neutral-500">@{user.username[0]}</span>
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
      <div class="partner-scroll-container">
        <div class="partner-scroll">
          {#each [...partners, ...partners] as partner (partner.url + Math.random())}
            <a
              class="partner-item {partner.bg
                ? partner.bg + ' border border-surface-300'
                : 'bg-white'}"
              target="_blank"
              href={partner.url}
            >
              <img
                class="mx-auto h-10"
                src={partner.image}
                alt={partner.title}
              />
            </a>
          {/each}
        </div>
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
            <p class="mt-2 text-pretty text-neutral-300">{saying.message}</p>
          </a>
        {/each}
      </div>
    </div>

    <footer id="page-footer" class="px-4 pb-24 pt-12 text-surface-400">
      <div class="flex h-16 flex-col items-center">
        <p class="flex flex-row items-center gap-1">
          <span class="text-sm">© 2024</span>
          <a class="" href="https://panda.fans" target="_blank"
            ><img
              class="w-28"
              src="/_assets/icpanda-dao-white.svg"
              alt="ICPanda DAO"
            /></a
          >
        </p>
        <p class="mt-2 text-center text-sm capitalize antialiased">
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
<MoreMenuPopup target="popupNavigationMore" />

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
    background-size: 1734px auto;
    animation: slideBackground 60s linear infinite;
  }

  @keyframes slideBackground {
    from {
      background-position: 0 0;
    }
    to {
      background-position: -1734px 0;
    }
  }

  :global(.rainbow-button) {
    position: relative;
    border-radius: 9999px;
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
    background: radial-gradient(circle, white 61%, rgba(255, 255, 255, 1) 100%);
    border-radius: 9999px;
    filter: blur(1px);
    z-index: 1;
    transition: all 0.3s ease;
  }

  :global(.rainbow-button:hover::after) {
    inset: 5px;
  }

  @keyframes move-gradient {
    0% {
      background-position: 0% 50%;
    }
    100% {
      background-position: 200% 50%;
    }
  }

  .partner-scroll-container {
    width: 100%;
    overflow: hidden;
    padding: 10px 0;
  }

  @keyframes scroll {
    0% {
      transform: translateX(0);
    }
    100% {
      transform: translateX(-50%);
    }
  }

  .partner-scroll {
    display: flex;
    width: max-content;
    animation: scroll 30s linear infinite;
  }

  .partner-item {
    flex: 0 0 auto;
    min-width: 160px;
    max-width: 250px;
    margin-right: 20px;
    padding: 10px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .partner-item img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  /* 鼠标悬停时暂停动画 */
  .partner-scroll-container:hover .partner-scroll {
    animation-play-state: paused;
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
