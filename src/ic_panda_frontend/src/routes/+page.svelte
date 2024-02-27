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

</script>

<div class="mt-24 px-4 flex flex-col flex-nowrap content-center items-center">
  <div class="flex flex-col items-center">
    <Avatar
      class="hover:scale-150 transition duration-700 ease-in-out"
      rounded="rounded-full"
      src="/_assets/logo.svg"
      alt="ICPanda logo"
      width="w-[100px]"
    />

    <img class="mt-5" src="/_assets/icpanda-dao.svg" alt="ICPanda brand"/>
  </div>

  <div class="mt-5 max-w-4xl">
    <p class="text-lg antialiased font-normal text-center capitalize">ICPanda DAO represents a Decentralized Autonomous Organization (DAO) committed to advancing the Panda brand within the decentralized ecosystem of the Internet Computer. As a DAO, it operates with a community-driven approach, leveraging the Internet Computer's blockchain technology to foster an environment of transparency, autonomy, and collaborative decision-making. This initiative aims to cultivate and expand the Panda brand's presence and influence in a decentralized manner.</p>
  </div>

  <div class="mt-10 space-x-4">
    <a type="button" class="btn variant-filled bg-slate-950" href="https://twitter.com/ICPandaDAO" target="_blank">
      <span><IconX /></span>
      <span class="text-left">
        Official Twitter
      </span>
    </a>
    <a type="button" class="btn variant-filled bg-slate-950" href="https://github.com/ldclabs/ic-panda" target="_blank">
      <span><IconGithub /></span>
      <span class="text-left">
        Github Repository
      </span>
    </a>
  </div>

  <!-- <section>Principal: {principal}</section>
  <ButtonIC on:click={handleSignIn}>
		<svelte:fragment slot="action">Connect with</svelte:fragment>
		Internet Identity
	</ButtonIC> -->
</div>

<div class="flex flex-col items-center absolute inset-x-0 bottom-10 h-16">
  <img class="mt-4 w-28" src="/_assets/icpanda-dao.svg" alt="ICPanda brand"/>
  <p class="mt-2 text-sm antialiased text-center capitalize text-slate-900/50">A decentralized Panda meme brand built on the Internet Computer.</p>
</div>