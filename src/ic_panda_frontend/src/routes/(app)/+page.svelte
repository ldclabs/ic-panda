<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import IconExchangeDollar from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import Saos from '$lib/components/ui/Saos.svelte'
  import { getTokenPrice, type TokenPrice } from '$lib/stores/exchange'
  import { getPriceNumber } from '$lib/utils/helper'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { ConicGradient, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'

  const toastStore = getToastStore()
  const icpPrice = getTokenPrice(ICPToken.canisterId, true)
  const pandaPrice = getTokenPrice(PANDAToken.canisterId, true)

  onMount(async () => {
    await new Promise((res) => setTimeout(res, 3000))
    const notifications = await luckyPoolAPI.notifications()

    for (const n of notifications) {
      toastStore.trigger({
        autohide: n.timeout != 0,
        timeout: n.timeout,
        hideDismiss: !n.dismiss,
        classes: 'bg-black',
        message: n.message
      })
    }
  })
</script>

{#snippet tokenPrice(price: TokenPrice)}
  {@const color =
    price.priceUSDChange > 0 ? 'text-primary-500' : 'text-error-500'}
  <div class="flex flex-row items-center justify-between gap-2">
    <span class="">{price.symbol}</span>
    <span class={color}>{'$' + getPriceNumber(price.priceUSD)}</span>
    <span class={color}>
      {(price.priceUSDChange > 0 ? '↑' : '') +
        price.priceUSDChange.toFixed(2) +
        '%'}</span
    >
  </div>
{/snippet}

<section
  class="mt-10 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-20 xl:mt-32"
>
  <div class="flex flex-col items-center space-y-8">
    <Saos
      animation={'scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both'}
    >
      <div
        class="size-24 rounded-full transition duration-700 ease-in-out *:size-24 hover:scale-110 hover:shadow-2xl hover:shadow-primary-500/20 sm:size-28 *:sm:size-28 lg:size-32 *:lg:size-32"
      >
        <IconPanda />
      </div>
    </Saos>

    <img
      class="mt-6 w-auto max-w-xs sm:max-w-sm lg:max-w-md"
      src="/_assets/icpanda-dao.svg"
      alt="ICPanda brand"
    />
  </div>

  <!-- 优化描述文本的可读性 -->
  <div class="mt-12 max-w-4xl">
    <p
      class="text-center text-lg font-light leading-relaxed text-gray/80 sm:text-xl lg:text-2xl"
    >
      <a
        class="font-semibold text-panda underline decoration-2 underline-offset-4 transition-colors hover:decoration-primary-400"
        href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
        target="_blank">ICPanda</a
      >
      is a technical panda fully running on the
      <a
        class="font-semibold text-secondary-400 underline decoration-2 underline-offset-4 transition-colors hover:decoration-secondary-300"
        href="https://internetcomputer.org/"
        target="_blank"
      >
        Internet Computer
      </a>
      blockchain, building chain-native infrastructures, Anda AI and dMsg.net
    </p>
  </div>

  <div
    class="mt-16 grid w-full max-w-4xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4"
  >
    <a
      type="button"
      title="Follow on Twitter"
      class="group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
      href="https://twitter.com/ICPandaDAO"
      target="_blank"
    >
      <span class="transition-transform group-hover:scale-110"><IconX /></span>
      <span class="text-left">Twitter</span>
    </a>
    <a
      type="button"
      title="View Source Code"
      class="group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
      href="https://github.com/ldclabs"
      target="_blank"
    >
      <span class="transition-transform group-hover:scale-110"
        ><IconGithub /></span
      >
      <span class="text-left">Github</span>
    </a>
    <a
      type="button"
      title="Exchange Tokens"
      class="group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
      href="https://app.icpswap.com/swap/pro?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
      target="_blank"
    >
      <span class="transition-transform group-hover:scale-110"
        ><IconExchangeDollar /></span
      >
      <span class="text-left">ICPSwap</span>
    </a>
    <a
      type="button"
      title="Exchange Tokens"
      class="group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
      href="https://www.kongswap.io/stats/druyg-tyaaa-aaaaq-aactq-cai"
      target="_blank"
    >
      <span class="transition-transform group-hover:scale-110"
        ><IconExchangeDollar /></span
      >
      <span class="text-left">KongSwap</span>
    </a>
  </div>
</section>

<section
  class="mt-20 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-32"
>
  <div class="mb-16 text-center">
    <h2
      class="h2 bg-gradient-to-r from-primary-400 to-secondary-400 bg-clip-text font-black uppercase tracking-wider text-transparent"
    >
      Tokenomics
    </h2>
    <p class="mx-auto mt-4 max-w-2xl text-lg text-gray/80">
      Discover the distribution and utility of PANDA tokens
    </p>
    <div
      class="m-auto mt-0 flex max-w-3xl flex-col justify-center bg-transparent p-2 md:flex-row md:gap-4"
    >
      {#if $icpPrice}
        {@render tokenPrice($icpPrice)}
      {/if}
      {#if $pandaPrice}
        {@render tokenPrice($pandaPrice)}
      {/if}
    </div>
  </div>
  <div
    class="grid w-full max-w-4xl grid-cols-1 gap-12 lg:grid-cols-2 lg:gap-16"
  >
    <Saos
      once={false}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <div class="space-y-8 rounded-2xl p-8 backdrop-blur-sm">
        <div class="space-y-6 text-center">
          <div class="space-y-2">
            <h3 class="text-sm uppercase tracking-wide text-gray/80"
              >Token Name</h3
            >
            <p class="text-2xl font-bold text-panda">
              <a
                class="underline decoration-2 underline-offset-4 transition-colors hover:text-primary-400"
                title="ICPanda Token Info"
                href="https://www.coingecko.com/en/coins/icpanda-dao"
                target="_blank">ICPanda</a
              >
            </p>
          </div>

          <div class="space-y-2">
            <h3 class="text-sm uppercase tracking-wide text-gray/80"
              >Token Symbol</h3
            >
            <p class="text-2xl font-bold text-panda">
              <a
                class="underline decoration-2 underline-offset-4 transition-colors hover:text-primary-400"
                title="Buy PANDA Tokens"
                href="https://app.icpswap.com/swap/pro?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
                target="_blank">PANDA</a
              >
            </p>
          </div>

          <div class="space-y-2">
            <h3 class="text-sm uppercase tracking-wide text-gray/80"
              >Max Supply</h3
            >
            <p class="text-2xl font-bold text-panda">1,080,000,000</p>
          </div>
        </div>
      </div>
    </Saos>
    <Saos
      once={false}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <div class="flex flex-col items-center">
        <ConicGradient
          legend
          width="w-48 h-48"
          regionCone="hover:scale-105 hover:shadow-2xl hover:shadow-primary-500/20 transition-all duration-500 ease-in-out"
          stops={[
            {
              label: 'Dev Team',
              color: 'rgba(15,186,129,0.8)',
              start: 0,
              end: 4
            },
            {
              label: 'Seed Funders',
              color: 'rgba(79,70,229,0.8)',
              start: 4,
              end: 8
            },
            {
              label: 'SNS Swap',
              color: 'rgba(212,25,118,0.8)',
              start: 8,
              end: 20
            },
            {
              label: 'Airdropped to holders',
              color: 'rgba(234,179,8,0.5)',
              start: 20,
              end: 52
            },
            {
              label: 'DAO Treasury - Airdrop to new users',
              color: 'rgba(234,179,8,0.6)',
              start: 52,
              end: 62
            },
            {
              label: 'DAO Treasury - Community Incentive',
              color: 'rgba(234,179,8,0.7)',
              start: 62,
              end: 80
            },
            {
              label: 'DAO Treasury - CEX Incentive',
              color: 'rgba(234,179,8,0.8)',
              start: 80,
              end: 90
            },
            {
              label: 'DAO Treasury - DEX Liquidity',
              color: 'rgba(234,179,8,0.9)',
              start: 90,
              end: 100
            }
          ]}
        >
          <h3 class="h3 mb-4 text-center font-bold">Token Distribution</h3>
        </ConicGradient>
      </div>
    </Saos>
  </div>
  <Saos
    once={false}
    animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
  >
    <div class="mt-20 w-full max-w-4xl">
      <div class="text-center">
        <h3
          class="h3 bg-gradient-to-r from-primary-400 to-secondary-400 bg-clip-text font-bold text-transparent"
        >
          Token Utility
        </h3>
        <p class="mt-4 text-lg leading-relaxed">
          <span class="font-bold text-panda">PANDA</span>
          is the only token issued by ICPanda DAO. By holding PANDA tokens, users
          can participate in:
        </p>
      </div>

      <div class="mt-4 grid grid-cols-1 gap-6 md:grid-cols-2">
        <div
          class="rounded-xl border border-gray/20 bg-gray/10 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex items-start gap-4">
            <span class="variant-soft-primary badge-icon p-3 text-lg font-bold"
              >1</span
            >
            <div>
              <h4 class="mb-2 font-semibold text-primary-500">DAO Governance</h4
              >
              <p class="text-gray/80"
                >Participate in governance decisions and receive rewards</p
              >
            </div>
          </div>
        </div>
        <div
          class="rounded-xl border border-gray/20 bg-gray/10 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex items-start gap-4">
            <span
              class="variant-soft-secondary badge-icon p-3 text-lg font-bold"
              >2</span
            >
            <div>
              <h4 class="mb-2 font-semibold text-secondary-500"
                >Web3 Consumption</h4
              >
              <p class="text-gray-400"
                >Purchasing ICPanda Web3 apps, such as Anda.AI, dMsg.net and
                more</p
              >
            </div>
          </div>
        </div>
      </div>
    </div>
  </Saos>
</section>

<section
  class="mt-20 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-60"
  id="anda-ai"
>
  <Saos
    once={true}
    animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
  >
    <div
      class="m-auto flex max-w-4xl flex-col items-center justify-center gap-10 px-4"
    >
      <div class="m-auto rounded-sm">
        <img class="h-24 w-auto" src="/_assets/anda.svg" alt="Anda AI brand" />
      </div>
      <div class="flex flex-col items-center gap-10">
        <h2 class="h2"
          ><span class="text-secondary-500">Build the Future of AI Agents</span
          ><br /><span class="text-primary-500"
            >with Autonomous, Collaborative Intelligence</span
          ></h2
        >

        <p
          class="text-center text-lg font-light leading-relaxed text-gray/80 sm:text-xl lg:text-2xl"
          >Create next-generation AI agents with persistent memory,
          decentralized trust, and swarm intelligence</p
        >
        <div class="w-full">
          <a
            type="button"
            class="rainbow-button bg-black-950 group relative m-auto block w-64 overflow-hidden px-6 py-3 text-center text-white transition-all duration-300 ease-in-out hover:scale-105 active:scale-95"
            target="_blank"
            href="https://anda.ai"
          >
            <span class="relative z-10 text-lg">Explore Anda.AI</span>
            <span class="rainbow-border"></span>
          </a>
        </div>
      </div>
    </div>
  </Saos>
  <Saos
    once={false}
    animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
  >
    <div
      class="m-auto mt-10 flex max-w-4xl flex-col items-center justify-center gap-10 px-4 lg:mt-20"
    >
      <h2
        class="h2 bg-gradient-to-r from-primary-500 to-secondary-500 bg-clip-text font-black text-transparent"
        >Anda AI: Foundation for Agent Civilization</h2
      >

      <div class="flex w-full flex-col gap-10">
        <div
          class="rounded-xl border border-gray/20 bg-gray/10 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex flex-col items-center justify-between sm:flex-row">
            <h3 class="h3">ANDA Protocol</h3>
            <a
              href="https://github.com/ldclabs/anda"
              class="font-bold text-secondary-500 underline underline-offset-4"
              target="_blank">github.com/ldclabs/anda</a
            >
          </div>
          <p class="text-gray/80"
            >Rust framework for building evolvable agents</p
          >
          <ul class="mt-2 flex flex-col gap-2">
            <li
              ><span class="pr-2">🪪</span><span
                >ICP Blockchain-based persistent identity system</span
              ></li
            >
            <li
              ><span class="pr-2">🧩</span><span
                >Composable Intelligence - Mix-and-match agents</span
              ></li
            >
            <li
              ><span class="pr-2">📡</span><span
                >Agent to Agent: Securely collaborate across ecosystems</span
              ></li
            >
          </ul>
        </div>

        <div
          class="rounded-xl border border-gray/20 bg-gray/10 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex flex-col items-center justify-between sm:flex-row">
            <h3 class="h3">ANDA DB</h3>
            <a
              href="https://github.com/ldclabs/anda-db"
              class="font-bold text-secondary-500 underline underline-offset-4"
              target="_blank">github.com/ldclabs/anda-db</a
            >
          </div>
          <p class="text-gray/80">Agent memory reimagined</p>
          <ul class="mt-2 flex flex-col gap-2">
            <li
              ><span class="pr-2">🔍</span><span
                >Multi-modal data & hybrid search (BTree + TFS + HNSW)</span
              ></li
            >
            <li
              ><span class="pr-2">🔐</span><span
                >Encrypted storage with blockchain backed provenance</span
              ></li
            >
            <li
              ><span class="pr-2">🧠</span><span
                >Continuous knowledge accretion across lifetimes</span
              ></li
            >
          </ul>
        </div>

        <div
          class="rounded-xl border border-gray/20 bg-gray/10 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex flex-col items-center justify-between sm:flex-row">
            <h3 class="h3">ANDA Cloud</h3>
            <a
              href="https://github.com/ldclabs/anda-cloud"
              class="font-bold text-secondary-500 underline underline-offset-4"
              target="_blank">github.com/ldclabs/anda-cloud</a
            >
          </div>
          <p class="text-gray/80">Decentralized agent infrastructure</p>
          <ul class="mt-2 flex flex-col gap-2">
            <li
              ><span class="pr-2">🌐</span><span
                >On-chain agent discovery & reputation system</span
              ></li
            >
            <li
              ><span class="pr-2">💸</span><span
                >X402 protocol for machine to machine payments</span
              ></li
            >
            <li
              ><span class="pr-2">🛡️</span><span
                >TEE-verified agent interactions</span
              ></li
            >
          </ul>
        </div>
      </div>
    </div>
  </Saos>
</section>

<section
  class="mt-20 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-60"
  id="dmsg-net"
>
  <Saos
    once={false}
    animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
  >
    <div
      class="m-auto flex max-w-4xl flex-col items-center justify-center gap-10 px-4"
    >
      <img
        class="w-full max-w-[530px]"
        src="/_assets/icpanda-message.black.webp"
        alt="ICPanda message brand"
      />
      <p
        class="text-center text-lg font-light leading-relaxed text-gray/80 sm:text-xl lg:text-2xl"
      >
        ICPanda Message (dMsg.net) is a decentralized end-to-end encrypted
        messaging application fully running on the <a
          class="font-bold text-secondary-500 underline underline-offset-4"
          href="https://internetcomputer.org"
          target="_blank"
        >
          Internet Computer
        </a> blockchain
      </p>
      <div class="w-full">
        <a
          type="button"
          class="rainbow-button group relative m-auto block w-64 overflow-hidden px-6 py-3 text-center text-white transition-all duration-300 ease-in-out hover:scale-105 hover:bg-secondary-700 active:scale-95"
          target="_blank"
          href="https://dmsg.net"
        >
          <span class="relative z-10 text-lg">Launch dMsg.net app</span>
          <span class="rainbow-border"></span>
        </a>
      </div>
    </div>

    <div
      class="m-auto mt-10 flex max-w-3xl flex-col items-center justify-center gap-6 px-4 text-black md:mt-20"
    >
      <h2
        class="h2 bg-gradient-to-r from-secondary-900 to-primary-900 bg-clip-text font-black text-transparent"
        >dMsg: Safe, Private, Decentralized</h2
      >
      <div class="divide-neutral-500 flex flex-col gap-8 divide-y md:gap-10">
        <div class="pt-0">
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
        <div class="pt-8 md:pt-10">
          <h3 class="h3"
            ><span class="pr-2 text-5xl">💬</span>Multi-user Chats</h3
          >
          <p class="text-neutral-300 mt-4">
            Message channels support one-to-many chats, where a manager can add
            or remove members and exchange encryption keys. If the last manager
            leaves the channel, all messages in the channel are deleted.
          </p>
        </div>
        <div class="pt-8 md:pt-10">
          <h3 class="h3"
            ><span class="pr-2 text-5xl">⛏</span>Proof of Link (PoL) Mining</h3
          >
          <p class="text-neutral-300 mt-4">
            Fairly mint DMSG through
            <a
              class="underline underline-offset-4"
              href="https://github.com/ldclabs/ic-panda/tree/main/src/ic_dmsg_minter"
              target="_blank">Proof of Link (PoL)</a
            >, rewards users for creating verified connections, fostering
            genuine engagement and decentralization.
          </p>
        </div>
        <div class="pt-8 md:pt-10">
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
</section>

<footer id="page-footer" class="mt-20 flex-none lg:mt-60">
  <PageFooter />
</footer>

<style>
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
      transform: scale(1.5);
    }
    100% {
      transform: scale(1);
    }
  }

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
    background: rgb(var(--color-surface-900));
    border-radius: 9999px;
    z-index: 1;
    transition: all 0.3s ease;
  }

  :global(.rainbow-button:hover::after) {
    background: rgb(var(--color-secondary-700) / var(--tw-bg-opacity, 1));
  }

  :global(.rainbow-button:hover::after) {
    inset: 3px;
  }

  :global(.card:hover) {
    transform: translateY(-4px);
    transition: all 0.3s ease;
  }

  :global(.btn:hover) {
    transform: translateY(-2px);
    transition: all 0.2s ease;
  }

  :global(a:hover) {
    transition: all 0.2s ease;
  }

  :global(.backdrop-blur-sm) {
    backdrop-filter: blur(8px);
  }

  :global(.bg-clip-text) {
    -webkit-background-clip: text;
    background-clip: text;
  }

  @keyframes move-gradient {
    0% {
      background-position: 0% 50%;
    }
    50% {
      background-position: 100% 50%;
    }
    100% {
      background-position: 0% 50%;
    }
  }
</style>
