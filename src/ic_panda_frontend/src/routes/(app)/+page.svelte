<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import IconAndaLogo from '$lib/components/icons/IconAndaLogo.svelte'
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
  class="mt-10 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-20"
>
  <div class="flex flex-col items-center">
    <Saos
      animation={'scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both'}
    >
      <div
        class="size-24 rounded-full transition duration-700 ease-in-out *:size-24 hover:scale-150 hover:shadow-lg"
      >
        <IconPanda />
      </div>
    </Saos>

    <img class="mt-6" src="/_assets/icpanda-dao.svg" alt="ICPanda brand" />
  </div>

  <div class="mt-10 max-w-4xl">
    <p class="text-center text-lg font-normal antialiased">
      <a
        class="font-bold text-panda underline underline-offset-4"
        href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
        target="_blank">ICPanda</a
      >
      is technical panda fully running on the
      <a
        class="font-bold text-secondary-500 underline underline-offset-4"
        href="https://internetcomputer.org/"
        target="_blank"
      >
        Internet Computer
      </a>
      blockchain, building chain-native infrastructures, Anda AI and dMsg.net
    </p>
  </div>

  <div
    class="mt-10 flex max-w-4xl flex-row items-center gap-6 max-sm:flex-col *:max-sm:w-60"
  >
    <a
      type="button"
      title="Follow on Twitter"
      class="bg-slate-950 variant-filled btn"
      href="https://twitter.com/ICPandaDAO"
      target="_blank"
    >
      <span><IconX /></span>
      <span class="text-left">Twitter</span>
    </a>
    <a
      type="button"
      title="View Source Code"
      class="bg-slate-950 variant-filled btn"
      href="https://github.com/ldclabs"
      target="_blank"
    >
      <span class="*:scale-125"><IconGithub /></span>
      <span class="text-left">Github</span>
    </a>
    <a
      type="button"
      title="Exchange Tokens"
      class="bg-slate-950 variant-filled btn"
      href="https://app.icpswap.com/swap/pro?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
      target="_blank"
    >
      <span><IconExchangeDollar /></span>
      <span class="text-left">ICPSwap</span>
    </a>
    <a
      type="button"
      title="Exchange Tokens"
      class="bg-slate-950 variant-filled btn"
      href="https://www.kongswap.io/stats/druyg-tyaaa-aaaaq-aactq-cai"
      target="_blank"
    >
      <span><IconExchangeDollar /></span>
      <span class="text-left">KongSwap</span>
    </a>
  </div>
  <div
    class="m-auto flex max-w-3xl flex-col bg-transparent p-2 md:flex-row md:gap-4"
  >
    {#if $icpPrice}
      {@render tokenPrice($icpPrice)}
    {/if}
    {#if $pandaPrice}
      {@render tokenPrice($pandaPrice)}
    {/if}
  </div>
</section>

<section
  class="mt-10 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-20"
>
  <h2 id="tokenomics" class="h2 font-extrabold uppercase">Tokenomics</h2>
  <div class="mt-8 flex w-full flex-col justify-evenly gap-y-4 sm:flex-row">
    <Saos
      once={false}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <div class="flex flex-col gap-4 text-center">
        <h3 class="h3 font-bold">
          <p>Token Name</p>
          <p class="text-panda"
            ><a
              class="underline underline-offset-4"
              title="ICPanda Token Info"
              href="https://www.coingecko.com/en/coins/icpanda-dao"
              target="_blank">ICPanda</a
            ></p
          >
        </h3>
        <h3 class="h3 font-bold">
          <p>Token Symbol</p>
          <p class="text-panda"
            ><a
              class="underline underline-offset-4"
              title="Buy PANDA Tokens"
              href="https://app.icpswap.com/swap/pro?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
              target="_blank">PANDA</a
            >
          </p>
        </h3>
        <h3 class="h3 font-bold">
          <p>Max Supply</p>
          <p class="text-panda">1,080,000,000</p>
        </h3>
      </div>
    </Saos>
    <Saos
      once={false}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <ConicGradient
        legend
        width="w-36"
        regionCone="hover:scale-125 hover:shadow-lg hover:-rotate-12 transition duration-700 ease-in-out"
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
        <h3 class="h3 font-bold">Token Distribution</h3>
      </ConicGradient>
    </Saos>
  </div>
  <Saos
    once={false}
    animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
  >
    <div class="mt-12 gap-4">
      <h3 class="h3 text-center font-bold">
        <p>Token utility</p>
      </h3>
      <div class="mt-4 max-w-screen-sm text-lg font-normal antialiased">
        <p>
          <span class="font-bold text-panda">PANDA</span>
          is the only token issued by ICPanda DAO. By holding PANDA tokens, users
          can participate in:
        </p>
        <ol class="list mt-2">
          <li>
            <span class="variant-soft-primary badge-icon p-4">1</span>
            <span class="flex-auto">
              Governance decisions of ICPanda DAO and receive rewards
            </span>
          </li>
          <li>
            <span class="variant-soft-primary badge-icon p-4">2</span>
            <span class="flex-auto">Participate in PANDA Prize</span>
          </li>
          <li>
            <span class="variant-soft-primary badge-icon p-4">3</span>
            <span class="flex-auto"
              >Purchasing ICPanda Web3 apps, such as dMsg.net, Anda.AI</span
            >
          </li>
          <li>
            <span class="variant-soft-primary badge-icon p-4">4</span>
            <span class="flex-auto">
              Purchasing other Web3 infrastructure services in the future
            </span>
          </li>
        </ol>
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
      <div
        class="m-auto rounded-sm transition duration-700 ease-in-out *:h-24 *:w-auto hover:scale-125 hover:shadow-lg"
      >
        <IconAndaLogo />
      </div>
      <div class="flex flex-col items-center gap-10 antialiased">
        <h2 class="h2"
          ><span>Build the Future of AI Agents</span><br /><span
            class="text-panda">with Autonomous, Collaborative Intelligence</span
          ></h2
        >

        <p class="text-lg"
          >Create next-generation AI agents with persistent memory,
          decentralized trust, and swarm intelligence</p
        >
        <div class="w-full">
          <a
            type="button"
            class="rainbow-button bg-slate-950 group relative m-auto block w-64 overflow-hidden px-6 py-3 text-center text-white transition-all duration-300 ease-in-out hover:scale-105 active:scale-95"
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
      class="m-auto mt-10 flex max-w-4xl flex-col items-center justify-center gap-10 px-4 antialiased lg:mt-20"
    >
      <h2 class="h2">Anda AI: Foundation for Agent Civilization</h2>

      <div class="flex flex-row flex-wrap justify-center gap-6">
        <div class="bg-indigo-950/10 card flex w-64 flex-col gap-4 p-4">
          <h3 class="h3">ANDA Protocol</h3>
          <a
            href="https://github.com/ldclabs/anda"
            class="font-bold text-secondary-500 underline underline-offset-4"
            target="_blank">github.com/ldclabs/anda</a
          >

          <p class="text-gray/80"
            >Rust framework for building evolvable agents</p
          >
          <ul class="flex flex-col gap-2">
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

        <div class="bg-indigo-950/10 card flex w-64 flex-col gap-4 p-4">
          <h3 class="h3">ANDA DB</h3>
          <a
            href="https://github.com/ldclabs/anda-db"
            class="font-bold text-secondary-500 underline underline-offset-4"
            target="_blank">github.com/ldclabs/anda-db</a
          >
          <p class="text-gray/80">Agent memory reimagined</p>
          <ul class="flex flex-col gap-2">
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

        <div class="bg-indigo-950/10 card flex w-64 flex-col gap-4 p-4">
          <h3 class="h3">ANDA Cloud</h3>
          <a
            href="https://github.com/ldclabs/anda-cloud"
            class="font-bold text-secondary-500 underline underline-offset-4"
            target="_blank">github.com/ldclabs/anda-cloud</a
          >
          <p class="text-gray/80">Decentralized agent infrastructure</p>
          <ul class="flex flex-col gap-2">
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
      <p class="text-xl font-normal antialiased">
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
          class="rainbow-button bg-slate-950 group relative m-auto block w-64 overflow-hidden px-6 py-3 text-center text-white transition-all duration-300 ease-in-out hover:scale-105 active:scale-95"
          target="_blank"
          href="https://dmsg.net"
        >
          <span class="relative z-10 text-lg">Launch dMsg.net app</span>
          <span class="rainbow-border"></span>
        </a>
      </div>
    </div>

    <div
      class="m-auto mt-10 flex max-w-4xl flex-col items-center justify-center gap-6 px-4 text-black antialiased md:mt-20"
    >
      <h2 class="h2">dMsg: Foundation for security and privacy</h2>
      <div class="divide-neutral-500 flex flex-col gap-8 divide-y md:gap-10">
        <div class="pt-10">
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
</style>
