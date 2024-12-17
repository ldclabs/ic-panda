<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import AirdropCard from '$lib/components/core/AirdropCard.svelte'
  import LuckyPoolChart from '$lib/components/core/LuckyPoolChart.svelte'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import PrizeCard from '$lib/components/core/PrizeCard.svelte'
  import IconExchangeDollar from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconLink from '$lib/components/icons/IconLink.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import { authStore } from '$lib/stores/auth'
  import { getTokenPrice, type TokenPrice } from '$lib/stores/exchange'
  import { getPriceNumber } from '$lib/utils/helper'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { ConicGradient, getToastStore } from '@skeletonlabs/skeleton'
  import Saos from 'saos'
  import { onMount } from 'svelte'

  const toastStore = getToastStore()
  const icpPrice = getTokenPrice(ICPToken.canisterId, true)
  const pandaPrice = getTokenPrice(PANDAToken.canisterId, true)

  onMount(async () => {
    await luckyPoolAPI.refreshAllState()

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

  $: principal = $authStore.identity.getPrincipal()
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

<div
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

  <div class="mt-6 max-w-4xl">
    <p class="text-center text-lg font-normal capitalize antialiased">
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
      blockchain. We are dedicated to build chain-native infrastructures and practical
      Web3 apps.
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
      href="https://github.com/ldclabs/ic-panda"
      target="_blank"
    >
      <span class="*:scale-125"><IconGithub /></span>
      <span class="text-left">Source Code</span>
    </a>
    <a
      type="button"
      title="Exchange Tokens"
      class="bg-slate-950 variant-filled btn"
      href="https://app.icpswap.com/swap/pro?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
      target="_blank"
    >
      <span><IconExchangeDollar /></span>
      <span class="text-left">Exchange</span>
    </a>
    <a
      type="button"
      title="Contact Us"
      class="bg-slate-950 variant-filled btn"
      href="https://dmsg.net/PANDA"
      target="_blank"
    >
      <span><IconLink /></span>
      <span class="text-left">dMsg.net</span>
    </a>
  </div>

  <div
    class="mt-10 flex w-full max-w-4xl flex-col flex-nowrap content-center items-center sm:mt-20"
  >
    <h2 id="tokenomics" class="h2 font-extrabold uppercase">Tokenomics</h2>
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
    <div class="mt-8 flex w-full flex-col justify-evenly gap-y-4 sm:flex-row">
      <Saos
        once={true}
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
        once={true}
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
      once={true}
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
                >Purchasing ICPanda Web3 apps, such as dMsg.net</span
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
  </div>

  {#key principal.toText()}
    <div
      class="mt-10 flex w-full max-w-4xl flex-col flex-nowrap content-center items-center sm:mt-20"
    >
      <h2 id="luckypool" class="h2 font-extrabold uppercase">Lucky Pool</h2>
      <div class="mt-8 w-full max-w-[820px]">
        <Saos
          once={true}
          animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
        >
          <AirdropCard />
        </Saos>
      </div>
      <div class="mt-6 w-full max-w-[820px]">
        <Saos
          once={true}
          animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
        >
          <PrizeCard />
        </Saos>
      </div>
      <div class="mt-6 w-full max-w-[820px]">
        <Saos
          once={true}
          animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
        >
          <LuckyPoolChart />
        </Saos>
      </div>
    </div>
  {/key}

  <footer id="page-footer" class="flex-none">
    <PageFooter />
  </footer>
</div>

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
</style>
