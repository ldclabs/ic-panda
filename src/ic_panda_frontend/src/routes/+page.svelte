<script lang="ts">
  import { onMount } from 'svelte'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import { Avatar, ConicGradient, getToastStore } from '@skeletonlabs/skeleton'
  import Saos from 'saos'

  const toastStore = getToastStore()

  onMount(async () => {
    await setTimeout(Promise.resolve, 3000)
    toastStore.trigger({
      timeout: 6000,
      hideDismiss: true,
      classes: 'bg-black',
      message:
        "<div class='flex flex-row gap-4 items-center'><div><img src='/_assets/images/celebrate.webp'></div><div><p class='text-lg'>We are upcoming on SNS</p><p class='text-sm'>The first decentralized meme brand in Web3.0</p></div><div><a class='bg-white btn btn-sm text-black' href='https://forum.dfinity.org/t/upcoming-icpanda-dao-launch-sns/27967/1' target='_blank'>Vote for US</a></div></div>"
    })
  })
</script>

<div
  id="home"
  class="mb-12 mt-12 flex flex-col flex-nowrap content-center items-center px-4 lg:mt-24"
>
  <div class="flex flex-col items-center">
    <Saos
      animation={'scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both'}
    >
      <Avatar
        class="transition duration-700 ease-in-out hover:scale-150 hover:shadow-lg"
        rounded="rounded-full"
        src="/_assets/logo.svg"
        alt="ICPanda logo"
        width="w-[100px]"
      />
    </Saos>

    <img class="mt-6" src="/_assets/icpanda-dao.svg" alt="ICPanda brand" />
  </div>

  <div class="mt-6 max-w-4xl">
    <p class="text-center text-lg font-normal capitalize antialiased">
      ICPanda DAO represents a Decentralized Autonomous Organization (DAO)
      committed to advancing the Panda meme brand within the decentralized
      ecosystem of the <a
        class="font-bold text-secondary-500"
        href="https://internetcomputer.org/"
        target="_blank"
      >
        Internet Computer
      </a>
      . As a DAO, it operates with a community-driven approach, leveraging the Internet
      Computer's blockchain technology to foster an environment of transparency,
      autonomy, and collaborative decision-making.
    </p>
  </div>

  <div class="mt-10 max-w-4xl space-x-4">
    <a
      type="button"
      class="bg-slate-950 variant-filled btn"
      href="https://twitter.com/ICPandaDAO"
      target="_blank"
    >
      <span><IconX /></span>
      <span class="text-left">Twitter</span>
    </a>
    <a
      type="button"
      class="bg-slate-950 variant-filled btn"
      href="https://github.com/ldclabs/ic-panda"
      target="_blank"
    >
      <span><IconGithub /></span>
      <span class="text-left">More Info</span>
    </a>
  </div>

  <div
    class="mt-12 flex w-full max-w-4xl flex-col flex-nowrap content-center items-center sm:mt-24"
  >
    <h2 id="luckypool" class="h2 font-extrabold uppercase">Lucky Pool</h2>
    <div class="mt-8 flex w-full flex-col justify-evenly gap-y-4 sm:flex-row">
      <Saos
        once={true}
        animation={'slide-top 0.6s cubic-bezier(.25,.46,.45,.94) both'}
      >
        <div class="relative">
          <img src="/_assets/images/luckypool-bg.webp" alt="Lucky Pool" />
          <div class="absolute top-1/2 mx-auto flex w-full">
            <a
              type="button"
              class="bg-slate-950 variant-filled btn m-auto"
              href="https://github.com/ldclabs/ic-panda/tree/main/src/ic_panda_luckypool"
              target="_blank"
            >
              <span><IconGithub /></span>
              <span class="text-left">Canister Source Code</span>
            </a>
          </div>
        </div>
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
            <p class="text-panda">ICPanda</p>
          </h3>
          <h3 class="h3 font-bold">
            <p>Token Symbol</p>
            <p class="text-panda">PANDA</p>
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
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>Project Launch</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span class="mr-2 text-panda"><IconCheckbox /></span>
              <span>Seed Fundraising</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
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
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>SNS Swap</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Launch Lucky Pool</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
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
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Launch Panda Badges System</span>
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
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Launch Panda NFT Platform</span>
            </p>
            <p
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
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
              class="card inline-flex flex-row content-center items-center bg-[#fff] p-6 shadow-md transition duration-700 ease-in-out hover:-translate-y-2 lg:px-8"
            >
              <span>Launch Panda Meme Brand Platform</span>
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
