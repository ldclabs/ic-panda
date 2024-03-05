<script lang="ts">
  import { getService } from "$lib/canisters/luckypool";
  import { onMount } from 'svelte';
  import type { Principal } from '@dfinity/principal';
  import { authStore } from '$lib/stores/auth'
  import { signIn } from '$lib/services/auth';
	// import ButtonIC from '$lib/components/ui/ButtonIC.svelte';
  import { Avatar } from '@skeletonlabs/skeleton';
  import IconGithub from '$lib/components/icons/IconGithub.svelte';
  import IconX from '$lib/components/icons/IconX.svelte';
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte';
  import { ConicGradient, getToastStore } from '@skeletonlabs/skeleton';

  const toastStore = getToastStore();
  let principal = "";

  async function updatePrincipal() {
    const identity = await authStore.getIdentity();
    const luckypool = await getService({identity: identity});
    const res: Principal = await luckypool.whoami();
    principal = res.toString();
  }

  onMount(async () => {

    await updatePrincipal();
  })

  async function handleSignIn() {
    await signIn({});
    await updatePrincipal();
  }

  toastStore.trigger({
    timeout: 10000,
    message: "<a href='https://forum.dfinity.org/t/upcoming-icpanda-dao-launch-sns/27967/1'  target='_blank'>Support Us: Upcoming ICPanda DAO launch SNS</a>",
  })

</script>

<div class="mt-12 lg:mt-24 mb-24 px-4 flex flex-col flex-nowrap content-center items-center">
  <div class="flex flex-col items-center">
    <Avatar
      class="hover:scale-150 hover:shadow-lg transition duration-700 ease-in-out"
      rounded="rounded-full"
      src="/_assets/logo.svg"
      alt="ICPanda logo"
      width="w-[100px]"
    />

    <img class="mt-5" src="/_assets/icpanda-dao.svg" alt="ICPanda brand"/>
  </div>

  <div class="mt-5 max-w-4xl">
    <p class="text-lg antialiased font-normal text-center capitalize">ICPanda DAO represents a Decentralized Autonomous Organization (DAO) committed to advancing the Panda meme brand within the decentralized ecosystem of the <a class="text-secondary-500 font-bold" href="https://internetcomputer.org/" target="_blank">Internet Computer</a>. As a DAO, it operates with a community-driven approach, leveraging the Internet Computer's blockchain technology to foster an environment of transparency, autonomy, and collaborative decision-making.</p>
  </div>

  <div class="mt-10 max-w-4xl space-x-4">
    <a type="button" class="btn variant-filled bg-slate-950" href="https://twitter.com/ICPandaDAO" target="_blank">
      <span><IconX /></span>
      <span class="text-left">
        Twitter
      </span>
    </a>
    <a type="button" class="btn variant-filled bg-slate-950" href="https://github.com/ldclabs/ic-panda" target="_blank">
      <span><IconGithub /></span>
      <span class="text-left">
        More Info
      </span>
    </a>
  </div>

  <div class="mt-12 sm:mt-24 max-w-4xl w-full flex flex-col flex-nowrap content-center items-center">
    <h2 class="h2 uppercase font-extrabold">Tokenomics</h2>
    <div class="w-full flex flex-col gap-y-3 sm:flex-row mt-8 justify-evenly">
      <div class="flex flex-col gap-3 text-center">
        <h3 class="h3 font-bold">
          <p>Token Name</p>
          <p class="text-panda mr-1">ICPanda</p>
        </h3>
        <h3 class="h3 font-bold">
          <p>Token Symbol</p>
          <p class="text-panda mr-1">PANDA</p>
        </h3>
        <h3 class="h3 font-bold">
          <p>Total Supply</p>
          <p class="text-panda mr-1">1,000,000,000</p>
        </h3>
      </div>
      <ConicGradient legend width="w-36" stops={[
        { label: 'Dev Team', color: 'rgba(15,186,129,0.8)', start: 0, end: 4 },
        { label: 'Seed Funders', color: 'rgba(79,70,229,0.8)', start: 4, end: 8 },
        { label: 'SNS Swap', color: 'rgba(212,25,118,0.8)', start: 8, end: 20 },
        { label: 'DAO Treasury - Lucky Pool', color: 'rgba(234,179,8,0.5)', start: 20, end: 70 },
        { label: 'DAO Treasury - Community Incentive', color: 'rgba(234,179,8,0.6)', start: 70, end: 80 },
        { label: 'DAO Treasury - CEX Incentive', color: 'rgba(234,179,8,0.7)', start: 80, end: 90 },
        { label: 'DAO Treasury - Development Fund', color: 'rgba(234,179,8,0.8)', start: 90, end: 100 }
    ]}><h3 class="h3 font-bold">Token Distribution</h3></ConicGradient>
    </div>
    <div class="gap-3 mt-12">
      <h3 class="h3 font-bold text-center">
        <p>Token utility</p>
      </h3>
      <div class="mt-3 max-w-screen-sm text-lg antialiased font-normal">
        <p><span class="text-panda font-bold">PANDA</span> is the only token issued by ICPanda DAO. By holding PANDA tokens, users can participate in:</p>
        <ol class="list mt-2">
          <li><span class="badge-icon p-4 variant-soft-primary">1</span><span class="flex-auto">Governance decisions of ICPanda DAO and receive rewards</span></li>
          <li><span class="badge-icon p-4 variant-soft-primary">2</span><span class="flex-auto">Purchasing panda badges</span></li>
          <li><span class="badge-icon p-4 variant-soft-primary">3</span><span class="flex-auto">Creation and trading of panda culture NFTs</span></li>
          <li><span class="badge-icon p-4 variant-soft-primary">4</span><span class="flex-auto">Activities on the panda meme brand platform</span></li>
        </ol>
      </div>
    </div>
  </div>

  <div class="mt-12 sm:mt-24 max-w-4xl w-full flex flex-col flex-nowrap content-center items-center">
    <h2 class="h2 uppercase font-extrabold">Roadmap</h2>
    <div class="mt-8 v-timeline max-w-full">
      <div class="relative ml-5 mt-8 mb-8 lg:mb-16">
        <img class="absolute top-[-16px] left-[-68px] hover:scale-150 transition duration-700 ease-in-out" src="/_assets/images/panda-badge-1.webp" alt="Panda badge">
        <h3 class="h3 font-bold ml-3"><span class="text-panda mr-1">Feb</span>2024</h3>
        <div class="flex flex-row gap-3 mt-3 p-3 overflow-x-auto">
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span class="text-panda mr-1"><IconCheckbox /></span>
            <span>Project Launch</span>
          </p>
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span class="text-panda mr-1"><IconCheckbox /></span>
            <span>Seed Fundraising</span>
          </p>
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span class="text-panda mr-1"><IconCheckbox /></span>
            <span>Website On IC</span>
          </p>
        </div>
      </div>
      <div class="relative ml-5 mt-8 mb-8 lg:mb-16">
        <img class="absolute top-[-16px] left-[-68px] hover:scale-150 transition duration-700 ease-in-out" src="/_assets/images/panda-badge-2.webp" alt="Panda badge">
        <h3 class="h3 font-bold ml-3"><span class="text-panda mr-1">Q1</span>2024</h3>
        <div class="flex flex-row gap-3 mt-3 p-3 overflow-x-auto">
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span>SNS Swap</span>
          </p>
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span>Launch Lucky Pool</span>
          </p>
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span>List PANDA Token on DEX</span>
          </p>
        </div>
      </div>
      <div class="relative ml-5 mt-8 mb-8 lg:mb-16">
        <img class="absolute top-[-16px] left-[-68px] hover:scale-150 transition duration-700 ease-in-out" src="/_assets/images/panda-badge-3.webp" alt="Panda badge">
        <h3 class="h3 font-bold ml-3"><span class="text-panda mr-1">Q2</span>2024</h3>
        <div class="flex flex-row gap-3 mt-3 p-3 overflow-x-auto">
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span>Launch Panda Badges System</span>
          </p>
        </div>
      </div>
      <div class="relative ml-5 mt-8 mb-8 lg:mb-16">
        <img class="absolute top-[-16px] left-[-68px] hover:scale-150 transition duration-700 ease-in-out" src="/_assets/images/panda-badge-4.webp" alt="Panda badge">
        <h3 class="h3 font-bold ml-3"><span class="text-panda mr-1">Q3</span>2024</h3>
        <div class="flex flex-row gap-3 mt-3 p-3 overflow-x-auto">
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span>Launch Panda NFT Platform</span>
          </p>
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span>List PANDA Token on CEX</span>
          </p>
        </div>
      </div>
      <div class="relative ml-5 mt-8 mb-8 lg:mb-16">
        <img class="absolute top-[-16px] left-[-68px] hover:scale-150 transition duration-700 ease-in-out" src="/_assets/images/panda-badge-5.webp" alt="Panda badge">
        <h3 class="h3 font-bold ml-3"><span class="text-panda mr-1">Q4</span>2024</h3>
        <div class="flex flex-row gap-3 mt-3 p-3 overflow-x-auto">
          <p class="card shadow-md bg-[#fff] p-6 lg:px-8 inline-flex flex-row content-center items-center hover:-translate-y-2 transition duration-700 ease-in-out">
            <span>Launch Panda Meme Brand Platform</span>
          </p>
        </div>
      </div>
    </div>
  </div>

  <!-- <section>Principal: {principal}</section>
  <ButtonIC on:click={handleSignIn}>
		<svelte:fragment slot="action">Connect with</svelte:fragment>
		Internet Identity
	</ButtonIC> -->
</div>

<div class="flex flex-col items-center inset-x-0 h-16 mt-10 mb-24">
  <img class="mt-4 w-28" src="/_assets/icpanda-dao.svg" alt="ICPanda brand"/>
  <p class="mt-2 text-sm antialiased text-center capitalize text-slate-900/50">A decentralized Panda meme brand built on the <a class="text-secondary-500 font-bold" href="https://internetcomputer.org/" target="_blank">Internet Computer</a>.</p>
</div>