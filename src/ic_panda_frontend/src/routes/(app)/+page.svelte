<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import AirdropCard from '$lib/components/core/AirdropCard.svelte'
  import LuckyPoolChart from '$lib/components/core/LuckyPoolChart.svelte'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import PrizeCard from '$lib/components/core/PrizeCard.svelte'
  import IconExchangeDollar from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconOpenChat from '$lib/components/icons/IconOpenChat.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import { authStore } from '$lib/stores/auth'
  import { ConicGradient, getToastStore } from '@skeletonlabs/skeleton'
  import Saos from 'saos'
  import { onMount } from 'svelte'

  const toastStore = getToastStore()

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

  {#key principal.toText()}
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
          <LuckyPoolChart />
        </Saos>
      </div>
    </div>
  {/key}

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
              label: 'DAO Treasury - Airdropped to holders',
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
              <span class="flex-auto">Purchasing E2EE messages service</span>
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
