<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import IconExchangeDollar from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import Saos from '$lib/components/ui/Saos.svelte'
  import { tokensPrice, type TokenPrice } from '$lib/stores/icpswap.svelte'
  import { getPriceNumber, pruneAddress } from '$lib/utils/helper'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { ConicGradient, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'

  const toastStore = getToastStore()
  const icpPrice = $derived(tokensPrice.get(ICPToken.canisterId))
  const pandaPrice = $derived(tokensPrice.get(PANDAToken.canisterId))

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
        '%'}
    </span>
  </div>
{/snippet}

<section
  class="relative isolate flex w-full flex-col flex-nowrap content-center items-center overflow-hidden px-4 pt-10 lg:mt-10 xl:mt-20"
>
  <div class="flex flex-col items-center space-y-8">
    <Saos
      animation={'scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both'}
    >
      <div
        class="glow-ring size-28 rounded-full transition duration-700 ease-in-out *:size-28 hover:scale-110 hover:shadow-2xl hover:shadow-primary-500/20 sm:size-32 *:sm:size-32 lg:size-36 *:lg:size-36"
      >
        <IconPanda />
      </div>
    </Saos>

    <a
      class=""
      href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai"
      target="_blank"
    >
      <img
        class="mt-2 w-auto max-w-xs sm:max-w-sm lg:max-w-md"
        src="/_assets/icpanda-dao.svg"
        alt="ICPanda brand"
      />
    </a>

    <h1
      class="neon-heading text-balance py-2 text-center text-3xl font-black leading-tight sm:text-4xl lg:text-6xl"
    >
      Chain‑Native Infrastructures for the AI Agent Era
    </h1>
  </div>

  <!-- 优化描述文本的可读性 -->
  <div class="mt-2 max-w-4xl">
    <p
      class="text-pretty text-center text-lg font-light leading-relaxed text-gray/80 sm:text-xl lg:text-2xl"
    >
      <span
        >We are building the open-source stack for agents to remember, transact,
        and evolve as first-class citizens in Web3.</span
      >
    </p>
  </div>

  <div
    class="mt-10 grid w-full max-w-4xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4"
  >
    <a
      type="button"
      title="Follow on Twitter"
      class="btn-neon group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
      href="https://twitter.com/ICPandaDAO"
      target="_blank"
    >
      <span class="transition-transform group-hover:scale-110"><IconX /></span>
      <span class="text-left">Twitter</span>
    </a>
    <a
      type="button"
      title="View Source Code"
      class="btn-neon group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
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
      class="btn-neon group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
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
      class="btn-neon group variant-filled btn transition-all duration-300 hover:bg-secondary-700 hover:shadow-lg hover:shadow-secondary-500/10"
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
    <a
      title="Exchange Tokens"
      class="m-auto mt-0 flex max-w-3xl flex-col justify-center rounded-lg bg-transparent p-2 transition-colors duration-300 ease-in-out hover:bg-primary-500/10 md:flex-row md:gap-4"
      href="https://app.icpswap.com/swap/pro?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
      target="_blank"
    >
      {#if icpPrice}
        {@render tokenPrice(icpPrice)}
      {/if}
      {#if pandaPrice}
        {@render tokenPrice(pandaPrice)}
      {/if}
    </a>
  </div>
  <div
    class="grid w-full max-w-4xl grid-cols-1 gap-12 lg:grid-cols-2 lg:gap-16"
  >
    <Saos
      once={false}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <div class="glass neon-border space-y-8 rounded p-8 backdrop-blur-sm">
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

          <div class="space-y-2">
            <h3 class="text-sm uppercase tracking-wide text-gray/80"
              >Contracts</h3
            >
            <p class="text"
              >ICP Chain: <a
                class="underline decoration-1 underline-offset-4 transition-colors hover:text-primary-400"
                title="PANDA contract on ICP chain"
                href="https://dashboard.internetcomputer.org/canister/druyg-tyaaa-aaaaq-aactq-cai"
                target="_blank">{pruneAddress('druyg-tyaaa-aaaaq-aactq-cai')}</a
              ></p
            >
            <p class="text"
              >BNB Chain: <a
                class="underline decoration-1 underline-offset-4 transition-colors hover:text-primary-400"
                title="PANDA contract on BNB chain"
                href="https://bscscan.com/token/0xe74583edaff618d88463554b84bc675196b36990"
                target="_blank"
                >{pruneAddress('0xe74583edAFF618D88463554b84Bc675196b36990')}</a
              ></p
            >
          </div>
        </div>
      </div>
    </Saos>
    <Saos
      once={false}
      animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
    >
      <div class="glass neon-border flex flex-col items-center rounded p-8">
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
          <h3 class="text-sm uppercase tracking-wide text-gray/80"
            >Token Distribution</h3
          >
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
          class="glass neon-border rounded border border-gray/20 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
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
          class="glass neon-border rounded border border-gray/20 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
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
  class="lg:mt-50 mt-20 flex flex-col flex-nowrap content-center items-center px-4 pt-10"
  id="anda-ai"
>
  <Saos
    once={true}
    animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
  >
    <div
      class="m-auto flex max-w-4xl flex-col items-center justify-center gap-10"
    >
      <div class="m-auto rounded-sm">
        <img class="h-24 w-auto" src="/_assets/anda.svg" alt="Anda AI brand" />
      </div>
      <div class="flex flex-col items-center gap-10">
        <h2
          class="h2 bg-gradient-to-r from-primary-400 to-secondary-400 bg-clip-text font-black uppercase tracking-wider text-transparent"
          >An Internet of Sovereign Minds</h2
        >

        <p
          class="text-center text-lg leading-relaxed text-gray/80 sm:text-xl lg:text-2xl"
          >We are building the open-source, decentralized foundation for AI
          agents that <b>remember</b> (KIP), <b>interact</b> (Anda), and
          <b>evolve</b>—creating a new nervous system for the web.</p
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
      class="m-auto mt-10 flex max-w-3xl flex-col items-center justify-center gap-10 lg:mt-20"
    >
      <div class="flex w-full flex-col gap-10">
        <div
          class="glass neon-border rounded border border-gray/20 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex flex-col items-center justify-between sm:flex-row">
            <h3 class="h3">Anda: Build More Than Agents. Build Ecosystems.</h3>
            <a
              href="https://github.com/ldclabs/anda"
              class="font-bold text-secondary-500 underline underline-offset-4"
              target="_blank">github.com/ldclabs/anda</a
            >
          </div>
          <p class="leading-relaxed text-gray/80"
            >Anda provides the open-source Rust runtime and cryptographic
            primitives to create complex, multi-agent systems that can securely
            collaborate, transact, and build shared intelligence.</p
          >
          <ul class="space-y-2 py-4 text-sm">
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Dynamic Skill Composition:</b> Build agents that learn and adapt
                by composing complex capabilities, not just calling static tools.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Effortless Multi-Agent Orchestration:</b> Deploy sophisticated
                swarm and market dynamics with built-in patterns, not complex custom
                code.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Cryptographic Identity:</b> Forge trustworthy agents with non-spoofable,
                on-chain identities rooted in ICP principals.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Privacy by Design:</b> Architected for TEEs, enabling confidential
                agent state and operations in a zero-trust world.</span
              ></li
            >
          </ul>
        </div>

        <div
          class="glass neon-border rounded border border-gray/20 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex flex-col items-center justify-between sm:flex-row">
            <h3 class="h3">KIP: The Language for AI Memory.</h3>
            <a
              href="https://github.com/ldclabs/KIP"
              class="font-bold text-secondary-500 underline underline-offset-4"
              target="_blank">github.com/ldclabs/KIP</a
            >
          </div>
          <p class="leading-relaxed text-gray/80"
            >An open, universal protocol that empowers AI agents to <b
              >Remember (KQL), Learn (KML), and Reflect (META)</b
            >. It transforms their internal world from a fleeting monologue into
            a persistent, structured dialogue.</p
          >
          <ul class="space-y-2 py-4 text-sm">
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Graph-Native Model:</b> Think in connections, not just documents.
                Unlocks complex, multi-hop reasoning impossible for standard RAG.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>LLM-Friendly Syntax:</b> Less hallucination, more precision.
                Drastically simplifies the prompt engineering for reliable memory
                interaction.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Auditable by Design:</b> From intention to action, every memory
                operation is a verifiable log entry. Trust becomes programmable.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Metabolic Knowledge Loop:</b> UPSERT isn't just a command; it's
                a heartbeat. Enables continuous learning, correction, and growth.</span
              ></li
            >
          </ul>
        </div>

        <div
          class="glass neon-border rounded border border-gray/20 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex flex-col items-center justify-between sm:flex-row">
            <h3 class="h3">Anda DB: The Polyglot Brain for Your AI.</h3>
            <a
              href="https://github.com/ldclabs/anda-db"
              class="font-bold text-secondary-500 underline underline-offset-4"
              target="_blank">github.com/ldclabs/anda-db</a
            >
          </div>
          <p class="leading-relaxed text-gray/80"
            >Anda DB is the first database that natively speaks every language
            of AI memory: <b
              >structured (BTree), semantic (HNSW), and symbolic (KIP)</b
            >. A single, high-performance Rust engine to power every thought.</p
          >
          <ul class="space-y-2 py-4 text-sm">
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>BTree Indexing:</b> For when precision is non-negotiable. Get
                lightning-fast, deterministic lookups on structured attributes and
                agent metadata.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>BM25 Full-Text Search:</b> Understand intent, not just keywords.
                Delivers superior relevance scoring with language-aware tokenization
                for natural language queries.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>HNSW Vector Search:</b> Think in concepts, not just text. Discover
                deeply related information through high-recall approximate similarity
                search on embeddings.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>KIP-Native Symbolic Graph:</b> The heart of the engine, not a
                bolt-on. Provides atomic UPSERT semantics for true knowledge evolution,
                making memory metabolic.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>Encrypted & Incremental Storage:</b> A brain that's both secure
                and fast. Protects memory with at-rest encryption while maintaining
                peak performance with non-blocking index updates.</span
              ></li
            >
          </ul>
        </div>

        <div
          class="glass neon-border rounded border border-gray/20 p-6 backdrop-blur-sm transition-all duration-300 hover:border-primary-500/50"
        >
          <div class="flex flex-col items-center justify-between sm:flex-row">
            <h3 class="h3">Anda Cloud: The Global Marketplace for AI Minds.</h3>
            <a
              href="https://github.com/ldclabs/anda-cloud"
              class="font-bold text-secondary-500 underline underline-offset-4"
              target="_blank">github.com/ldclabs/anda-cloud</a
            >
          </div>
          <p class="leading-relaxed text-gray/80"
            >Anda Cloud provides the decentralized public square where sovereign
            agents can be discovered, build reputation, collaborate, and
            transact—bootstrapping the world's first truly autonomous agent
            economy.</p
          >
          <ul class="space-y-2 py-4 text-sm">
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>The Uncensorable Town Square:</b> A global, on-chain registry
                where agents announce their existence, prove their liveness, and
                can be discovered by anyone, anywhere.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>The Web of Trust:</b> A sophisticated discovery engine that goes
                beyond keywords. Find the right collaborator based on their cryptographically
                proven capabilities and a transparent, on-chain reputation history.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>The Universal Banking Layer:</b> The final piece of the economic
                puzzle. We're building the rails for agents to seamlessly transact
                and escrow value across multiple chains, enabling a frictionless
                machine-to-machine economy.</span
              ></li
            >
            <li class="flex gap-2"
              ><span class="text-emerald-500">•</span><span
                ><b>The Open-Door Embassy:</b> Bridging nations of agents. Our gateway
                will allow agents from any protocol to connect with the Anda network,
                inheriting the security and economic primitives of our sovereign
                cloud.</span
              ></li
            >
          </ul>
        </div>
      </div>
    </div>
  </Saos>
</section>

<section
  class="lg:mt-50 mt-20 flex flex-col flex-nowrap content-center items-center px-4 pt-10"
  id="dmsg-net"
>
  <Saos
    once={false}
    animation="slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both"
  >
    <div
      class="m-auto flex max-w-4xl flex-col items-center justify-center gap-10"
    >
      <div class="m-auto rounded-sm">
        <img
          class="w-full max-w-[530px]"
          src="/_assets/icpanda-message.black.webp"
          alt="ICPanda message brand"
        />
      </div>
      <div class="flex flex-col items-center gap-10">
        <h2
          class="h2 bg-gradient-to-r from-primary-400 to-secondary-400 bg-clip-text font-black uppercase tracking-wider text-transparent"
        >
          Safe, Private, Decentralized</h2
        >

        <p
          class="text-center text-lg leading-relaxed text-gray/80 sm:text-xl lg:text-2xl"
          >ICPanda Message (dMsg.net) is the world's 1st <b
            >decentralized end-to-end encrypted</b
          > messaging application fully running on the blockchain.</p
        >
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
    </div>

    <div
      class="m-auto mt-10 flex max-w-3xl flex-col items-center justify-center gap-6 text-black md:mt-20"
    >
      <div class="divide-neutral-500 flex flex-col gap-8 divide-y md:gap-10">
        <div class="pt-0">
          <h3 class="h3"
            ><span class="pr-2 text-5xl">🔐</span>End-to-end Encryption</h3
          >
          <p class="text-neutral-300 mt-4">
            All user messages and files are encrypted client side using the
            <a
              class="underline underline-offset-4"
              href="https://datatracker.ietf.org/doc/html/rfc9052"
              target="_blank">RFC 9052 (COSE)</a
            >
            standard and
            <a
              class="underline underline-offset-4"
              href="https://internetcomputer.org/docs/building-apps/network-features/vetkeys/introduction"
              target="_blank">On-Chain vetKeys</a
            >, then stored permanently on the ICP blockchain. Decryption is
            possible only on the client side.
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
            Fairly mint $DMSG through
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
            It runs entirely as a smart contract on the ICP blockchain, governed
            by
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

  .glow-ring {
    position: relative;
  }
  .glow-ring::after {
    content: '';
    position: absolute;
    inset: -6px;
    border-radius: 9999px;
    background: conic-gradient(
      from 0deg,
      rgba(99, 102, 241, 0.4),
      rgba(16, 185, 129, 0.5),
      rgba(99, 102, 241, 0.4)
    );
    filter: blur(10px);
    z-index: -1;
    animation: spin-slow 12s linear infinite;
  }

  @keyframes spin-slow {
    to {
      transform: rotate(360deg);
    }
  }

  .neon-heading {
    background: linear-gradient(90deg, #22d3ee 0%, #a78bfa 50%, #34d399 100%);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    text-shadow:
      0 0 12px rgba(34, 211, 238, 0.15),
      0 0 24px rgba(167, 139, 250, 0.1);
  }

  .glass {
    background: linear-gradient(
      to bottom right,
      rgba(255, 255, 255, 0.08),
      rgba(255, 255, 255, 0.02)
    );
    box-shadow:
      0 10px 30px rgba(16, 185, 129, 0.08),
      inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }

  .neon-border {
    border: 1px solid rgba(34, 211, 238, 0.2);
    box-shadow:
      0 0 0 1px rgba(34, 211, 238, 0.08) inset,
      0 0 12px rgba(34, 211, 238, 0.08),
      0 0 32px rgba(167, 139, 250, 0.06);
  }

  .btn-neon {
    position: relative;
  }
  .btn-neon::after {
    content: '';
    position: absolute;
    inset: -2px;
    border-radius: 12px;
    background: linear-gradient(
      90deg,
      rgba(34, 211, 238, 0.35),
      rgba(167, 139, 250, 0.35)
    );
    filter: blur(8px);
    z-index: -1;
    opacity: 0;
    transition: opacity 0.2s ease;
  }
  .btn-neon:hover::after {
    opacity: 1;
  }
</style>
