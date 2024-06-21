<script lang="ts">
  import { LuckyPoolAPI, luckyPoolAPIAsync } from '$lib/canisters/luckypool'
  import AirdropCard from '$lib/components/core/AirdropCard.svelte'
  import LuckyDrawCard from '$lib/components/core/LuckyDrawCard.svelte'
  import LuckyPoolChart from '$lib/components/core/LuckyPoolChart.svelte'
  import PrizeCard from '$lib/components/core/PrizeCard.svelte'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconExchangeDollar from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconOpenChat from '$lib/components/icons/IconOpenChat.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import { ConicGradient, getToastStore } from '@skeletonlabs/skeleton'
  import Saos from 'saos'
  import { onMount } from 'svelte'

  const toastStore = getToastStore()

  let luckyPoolAPI: LuckyPoolAPI

  onMount(async () => {
    await new Promise((res) => setTimeout(res, 3000))

    luckyPoolAPI = await luckyPoolAPIAsync()
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

<div
  class="mb-12 mt-12 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-24"
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
        target="_blank">ICPanda DAO</a
      >
      is dedicated to building the Panda meme brand across the
      <a
        class="font-bold text-secondary-500 underline underline-offset-4"
        href="https://internetcomputer.org/"
        target="_blank"
      >
        Internet Computer's
      </a>
      decentralized ecosystem, enhancing the connection between pandas and humans.
      Our focus extends beyond the animals themselves, embracing all valuable and
      praiseworthy ideas, positioning the Panda meme as a symbol of cherished concepts
      globally.
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
      title="Join the Community"
      class="bg-slate-950 variant-filled btn"
      href="https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai"
      target="_blank"
    >
      <span><IconOpenChat /></span>
      <span class="text-left">Community</span>
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
      href="https://app.icpswap.com/swap?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
      target="_blank"
    >
      <span><IconExchangeDollar /></span>
      <span class="text-left">Exchange</span>
    </a>
  </div>

  <div
    class="mt-12 flex w-full max-w-4xl flex-col flex-nowrap content-center items-center sm:mt-24"
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
        <LuckyDrawCard />
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

  <div
    class="mt-12 flex w-full max-w-4xl flex-col flex-nowrap content-center items-center sm:mt-24"
  >
    <h2 id="tokenomics" class="h2 font-extrabold uppercase">Tokenomics</h2>
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
                href="https://info.icpswap.com/token/details/druyg-tyaaa-aaaaq-aactq-cai"
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
                href="https://app.icpswap.com/swap?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
                target="_blank">PANDA</a
              >
            </p>
          </h3>
          <h3 class="h3 font-bold">
            <p>Total Supply</p>
            <p class="text-panda">1,000,000,000</p>
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
              label: 'DAO Treasury - Lucky Pool to All',
              color: 'rgba(234,179,8,0.5)',
              start: 20,
              end: 70
            },
            {
              label: 'DAO Treasury - Community Incentive',
              color: 'rgba(234,179,8,0.6)',
              start: 70,
              end: 80
            },
            {
              label: 'DAO Treasury - CEX Incentive',
              color: 'rgba(234,179,8,0.7)',
              start: 80,
              end: 90
            },
            {
              label: 'DAO Treasury - DEX Liquidity',
              color: 'rgba(234,179,8,0.8)',
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
              <span class="flex-auto">Purchasing panda badges</span>
            </li>
            <li>
              <span class="variant-soft-primary badge-icon p-4">3</span>
              <span class="flex-auto">
                Creation and trading of panda culture NFTs
              </span>
            </li>
            <li>
              <span class="variant-soft-primary badge-icon p-4">4</span>
              <span class="flex-auto">
                Activities on the ICPanda meme brand platform
              </span>
            </li>
          </ol>
        </div>
      </div>
    </Saos>
  </div>

  <div
    class="mt-12 flex w-full max-w-4xl flex-col flex-nowrap content-center items-center sm:mt-24"
  >
    <h2 id="roadmap" class="h2 font-extrabold uppercase">Roadmap</h2>
    <div class="v-timeline mt-8 max-w-full">
      <Saos once={true} animation={'slide-top 0.6s ease-in-out both'}>
        <div class="relative mb-8 ml-4 mt-8 lg:mb-16">
          <img
            class="absolute left-[-64px] top-[-16px] transition duration-700 ease-in-out hover:scale-125"
            src="/_assets/images/panda-badge-1.webp"
            alt="Panda badge"
          />
          <h3 class="h3 ml-4 font-bold">
            <span class="mr-1 text-panda">Feb</span>
            2024
          </h3>
          <div class="mt-4 flex flex-row gap-4 overflow-x-auto p-4">
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>Project Launch</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>Seed Fundraising</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>Website On IC</span>
            </p>
          </div>
        </div>
      </Saos>
      <Saos once={true} animation={'slide-top 0.6s ease-in-out both'}>
        <div class="relative mb-8 ml-4 mt-8 lg:mb-16">
          <img
            class="absolute left-[-64px] top-[-16px] transition duration-700 ease-in-out hover:scale-125"
            src="/_assets/images/panda-badge-2.webp"
            alt="Panda badge"
          />
          <h3 class="h3 ml-4 font-bold">
            <span class="mr-1 text-panda">Q1</span>
            2024
          </h3>
          <div class="mt-4 flex flex-row gap-4 overflow-x-auto p-4">
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>SNS Swap</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>Launch Lucky Pool</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>List PANDA Token on DEX</span>
            </p>
          </div>
        </div>
      </Saos>
      <Saos once={true} animation={'slide-top 0.6s ease-in-out both'}>
        <div class="relative mb-8 ml-4 mt-8 lg:mb-16">
          <img
            class="absolute left-[-64px] top-[-16px] transition duration-700 ease-in-out hover:scale-125"
            src="/_assets/images/panda-badge-3.webp"
            alt="Panda badge"
          />
          <h3 class="h3 ml-4 font-bold">
            <span class="mr-1 text-panda">Q2</span>
            2024
          </h3>
          <div class="mt-4 flex flex-row gap-4 overflow-x-auto p-4">
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 text-gray/50 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Launch Panda Badges System (Suspend)</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Chain Fusion: <b>ckDOGE</b></span>
            </p>
          </div>
        </div>
      </Saos>
      <Saos once={true} animation={'slide-top 0.6s ease-in-out both'}>
        <div class="relative mb-8 ml-4 mt-8 lg:mb-16">
          <img
            class="absolute left-[-64px] top-[-16px] transition duration-700 ease-in-out hover:scale-125"
            src="/_assets/images/panda-badge-4.webp"
            alt="Panda badge"
          />
          <h3 class="h3 ml-4 font-bold">
            <span class="mr-1 text-panda">Q3</span>
            2024
          </h3>
          <div class="mt-4 flex flex-row gap-4 overflow-x-auto p-4">
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 text-gray/50 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Launch Panda NFT Platform (Suspend)</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Chain Fusion: More Chain-key Tokens</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>ICPanda AI: Panda Oracle</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>List PANDA Token on CEX</span>
            </p>
          </div>
        </div>
      </Saos>
      <Saos once={true} animation={'slide-top 0.6s ease-in-out both'}>
        <div class="relative mb-8 ml-4 mt-8 lg:mb-16">
          <img
            class="absolute left-[-64px] top-[-16px] transition duration-700 ease-in-out hover:scale-125"
            src="/_assets/images/panda-badge-5.webp"
            alt="Panda badge"
          />
          <h3 class="h3 ml-4 font-bold">
            <span class="mr-1 text-panda">Q4</span>
            2024
          </h3>
          <div class="mt-4 flex flex-row gap-4 overflow-x-auto p-4">
            <p
              class="card inline-flex flex-row content-center items-center bg-white p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Launch Panda Meme Brand Platform (SFT & VC)</span>
            </p>
          </div>
        </div>
      </Saos>
    </div>
  </div>
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
