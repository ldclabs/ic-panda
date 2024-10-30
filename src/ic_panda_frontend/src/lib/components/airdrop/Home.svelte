<script lang="ts">
  import {
    luckyPoolAPI,
    type Airdrops108Output
  } from '$lib/canisters/luckypool'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import { authStore } from '$lib/stores/auth'
  import { toastRun } from '$lib/stores/toast'
  import { getShortNumber, shortId } from '$lib/utils/helper'
  import { Principal } from '@dfinity/principal'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import Saos from 'saos'
  import { onMount } from 'svelte'

  const toastStore = getToastStore()
  const token_1 = 100000000n

  let user: Principal = $authStore.identity.getPrincipal()
  let userInput: string = user.isAnonymous() ? '' : user.toText()
  let airdropOutput: Airdrops108Output | null = null
  let validating = false

  async function getAirdropOutput() {
    airdropOutput = await luckyPoolAPI.airdrops108Of(user)
  }

  function getStatus(status: number) {
    switch (status) {
      case 0:
        return 'Not Started'
      case 1:
        return 'Started'
      case 2:
        return 'Finished'
      default:
        return 'Unknown'
    }
  }

  function checkInput() {
    if (userInput) {
      console.log('Checking input:', userInput)
      try {
        user = Principal.fromText(userInput.trim())
        validating = true
      } catch (_e) {
        validating = false
      }
    }
  }

  onMount(() => {
    const { abort } = toastRun(async () => {
      await getAirdropOutput()
    }, toastStore)
    return abort
  })
</script>

<div class="mt-12 flex max-w-4xl flex-1 flex-col px-4 md:mt-24">
  <Saos
    animation="scale-down-center 0.6s cubic-bezier(0.250, 0.460, 0.450, 0.940) both"
  >
    <div class="flex w-full flex-col items-center justify-center gap-4 px-4">
      <p class="text-2xl font-normal antialiased">
        An airdrop of <b>320 million PANDA</b> tokens to our loyal holders!
      </p>
      <ul class="">
        <li>
          <a
            class="underline underline-offset-4"
            href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/108"
            target="_blank"
          >
            Proposal 108:
          </a>
          <span
            >Neurons and wallets with over 10,000 PANDA will be eligible.
            Snapshot Date: Oct 31, 2024 (24:00 UTC).</span
          >
        </li>
        <li>
          <a
            class="underline underline-offset-4"
            href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/184"
            target="_blank"
          >
            Proposal 184:
          </a>
          <span
            >Disqualify any wallet with a decreased PANDA balance after the
            snapshot from receiving the airdrop.</span
          >
        </li>
      </ul>

      <div class="mt-8 w-full">
        <div
          class="input-group input-group-divider m-auto max-w-96 grid-cols-[1fr_auto]"
        >
          <input
            type="search"
            class="input truncate rounded-none border-0 !bg-white"
            bind:value={userInput}
            on:input={checkInput}
            placeholder="Enter principal ID ..."
          />
          <button
            class="variant-filled-primary cursor-pointer text-white disabled:cursor-not-allowed disabled:bg-primary-500/50"
            disabled={!validating}
            on:click={getAirdropOutput}>Check</button
          >
        </div>
      </div>
    </div>
  </Saos>
  <div
    class="card mx-auto mt-4 flex w-fit flex-col border-gray/10 bg-transparent p-4 *:justify-start"
  >
    {#if airdropOutput}
      <div class="text-gray/80 *:text-pretty *:break-all">
        <div class="*:text-pretty *:break-all">
          <p class="text-panda">
            Principal: {user.toText()}
          </p>
          {#if airdropOutput.airdrops.length == 0}
            <p>No airdrop available.</p>
          {:else}
            <p
              >Neurons airdrop processed? {airdropOutput.neurons_airdropped
                ? 'YES'
                : 'NO'}</p
            >
            <p
              >Ledger airdrop processed? {airdropOutput.ledger_airdropped
                ? 'YES'
                : 'NO'}</p
            >
            <ul class="mt-2">
              {#each airdropOutput.airdrops as airdrop}
                <li
                  >Airdrop weight: <span
                    >{getShortNumber(airdrop.weight / token_1)}</span
                  >
                  {#if airdrop.neuron_id[0]}
                    <span>
                      , from neuron: <a
                        class="underline underline-offset-4"
                        target="_blank"
                        href="https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/neuron/{airdrop
                          .neuron_id[0]}"
                      >
                        {shortId(airdrop.neuron_id[0], true)}.
                      </a>
                    </span>
                  {:else}
                    <span>, from ledger.</span>
                  {/if}
                  {#if airdrop.subaccount[0]}
                    <span>Subaccount: {airdrop.subaccount[0]}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
        <hr class="!border-t-1 mx-[-16px] my-2 !border-gray/10" />
        <p>Airdrop status: <b>{getStatus(airdropOutput.status)}</b></p>
        <p>
          Tokens per weight: <b>{airdropOutput.tokens_per_weight.toFixed(4)}</b>
        </p>
        <p>
          Tokens distributed: <b
            >{getShortNumber(airdropOutput.tokens_distributed / token_1)}</b
          >
        </p>
        <p class="mt-2">
          Total ledger weight: <b
            >{getShortNumber(airdropOutput.ledger_weight_total / token_1)}</b
          >
        </p>
        <p>Ledger snapshot hash: {airdropOutput.ledger_hash}</p>
        <p>
          Ledger snapshot updated at: {new Date(
            Number(airdropOutput.ledger_updated_at)
          ).toLocaleString()}
        </p>
        <p class="mt-2">
          Total neurons weight: <b
            >{getShortNumber(airdropOutput.neurons_weight_total / token_1)}</b
          >
        </p>
        <p>Neurons snapshot hash: {airdropOutput.neurons_hash}</p>
        <p>
          Neurons snapshot updated at: {new Date(
            Number(airdropOutput.neurons_updated_at)
          ).toLocaleString()}
        </p>
      </div>
    {/if}
  </div>
</div>

<footer id="page-footer" class="flex-none">
  <PageFooter />
</footer>
