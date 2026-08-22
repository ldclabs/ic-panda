<script lang="ts">
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
  import Reveal from '$lib/components/ui/Reveal.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import {
    GENESIS_SUPPLY,
    LINKS,
    PANDA_BNB_CONTRACT,
    PANDA_LEDGER_CANISTER_ID,
    PANDA_ROLES,
    TOKEN_FACTS
  } from '$lib/site'
  import { PANDAToken } from '$lib/utils/token'
  import { onMount } from 'svelte'
  import AllocationChart from './AllocationChart.svelte'
  import LinkOut from './LinkOut.svelte'
  import SectionShell from './SectionShell.svelte'

  // Read straight from the canonical ledger rather than restating a number
  // that goes stale. Whole tokens only — fractions are noise at this scale.
  let currentSupply: string | null = $state(null)

  onMount(async () => {
    try {
      const supply = await tokenLedgerAPI.totalSupply()
      const whole = supply / 10n ** BigInt(PANDAToken.decimals)
      currentSupply = whole.toLocaleString('en-US')
    } catch (err) {
      // The section falls back to sending people to the dashboard.
      console.warn('Failed to read PANDA total supply:', err)
    }
  })
</script>

<SectionShell id="panda" index="02" label="PANDA · Governance Token">
  <div class="grid grid-cols-1 gap-10 py-14 lg:grid-cols-12 lg:gap-12 lg:py-20">
    <div class="lg:col-span-5">
      <Reveal>
        <h2 class="display text-[clamp(2.25rem,5vw,3.5rem)]">
          One token.<br />One DAO.<br />Shared direction.
        </h2>
      </Reveal>
    </div>
    <div class="lg:col-span-7 lg:pt-3">
      <Reveal delay={80}>
        <p class="text-pretty text-lg leading-relaxed text-ink-70">
          PANDA is the governance and coordination token of ICPanda DAO. It
          connects community governance, treasury decisions, ecosystem
          incentives, and the projects we build in the open.
        </p>
      </Reveal>
    </div>
  </div>

  <!-- Supply: a fixed historical figure beside the live on-chain one -->
  <Reveal>
    <div class="grid grid-cols-1 border-t border-ink/15 sm:grid-cols-2">
      <div class="border-b border-ink/10 py-6 sm:border-b-0 sm:pr-6">
        <p class="eyebrow">Genesis Supply</p>
        <p class="mt-3 font-mono text-2xl font-medium tabular-nums md:text-3xl">
          {GENESIS_SUPPLY}
        </p>
        <p class="mt-1 font-mono text-xs tracking-[0.14em] text-ink-70">
          PANDA · at launch
        </p>
      </div>

      <div class="py-6 sm:border-l sm:border-ink/10 sm:pl-6">
        <p class="eyebrow">
          Current Supply
          <span
            class="ml-1.5 inline-block size-1.5 rounded-full align-middle"
            class:bg-panda={currentSupply}
            class:bg-ink-30={!currentSupply}
          ></span>
        </p>
        {#if currentSupply}
          <p
            class="mt-3 font-mono text-2xl font-medium tabular-nums md:text-3xl"
          >
            {currentSupply}
          </p>
          <p class="mt-1 font-mono text-xs tracking-[0.14em] text-ink-70">
            PANDA · live from the ledger
          </p>
        {:else}
          <p class="mt-3">
            <LinkOut
              href={LINKS.ledgerCanister}
              label="Verify on-chain"
              class="!text-base"
            />
          </p>
          <p class="mt-2 font-mono text-xs tracking-[0.14em] text-ink-70">
            PANDA · reading the ledger
          </p>
        {/if}
      </div>
    </div>

    <p class="mt-4 max-w-2xl font-mono text-xs leading-relaxed text-ink-70">
      Current supply moves with SNS voting rewards minted since genesis, net of
      tokens burned. It is queried directly from the ledger canister, not
      restated here.
    </p>
  </Reveal>

  <!-- Token facts -->
  <Reveal>
    <div class="mt-10 grid grid-cols-1 border-t border-ink/15 sm:grid-cols-3">
      {#each TOKEN_FACTS as fact, i (fact.label)}
        <div
          class="border-b border-ink/10 py-6 sm:border-b-0 sm:px-6 {i > 0
            ? 'sm:border-l sm:border-ink/10'
            : 'sm:pl-0'}"
        >
          <p class="eyebrow">{fact.label}</p>
          <p
            class="mt-3 font-mono text-xl font-medium tabular-nums md:text-2xl"
          >
            {fact.value}
          </p>
        </div>
      {/each}
    </div>
  </Reveal>

  <!-- Canonical ledger -->
  <Reveal>
    <div
      class="mt-10 flex flex-col gap-6 border border-ink/15 bg-white p-6 md:flex-row md:items-center md:justify-between md:p-8"
    >
      <div class="min-w-0">
        <p class="eyebrow">Canonical ICP Ledger</p>
        <div class="mt-3 flex items-center gap-2">
          <a
            class="link-mark break-all !text-sm sm:!text-base"
            href={LINKS.ledgerCanister}
            target="_blank"
            rel="noreferrer">{PANDA_LEDGER_CANISTER_ID}</a
          >
          <TextClipboardButton textValue={PANDA_LEDGER_CANISTER_ID} />
        </div>
      </div>
      <LinkOut
        href={LINKS.snsDashboard}
        label="View Official SNS Data"
        variant="outline"
        class="shrink-0"
      />
    </div>

    <p class="mt-4 max-w-2xl font-mono text-xs leading-relaxed text-ink-70">
      Supply, governance, proposals, neurons, and other live DAO data should
      always be verified on-chain.
    </p>
  </Reveal>

  <!-- Genesis allocation -->
  <div class="mt-16 border-t border-ink/15 pt-14 md:mt-24 md:pt-20">
    <div class="grid grid-cols-1 gap-10 lg:grid-cols-12 lg:gap-12">
      <div class="lg:col-span-5">
        <Reveal>
          <p class="eyebrow">Genesis Allocation</p>
          <h3 class="display mt-5 text-[clamp(2rem,4.2vw,3rem)]">
            Built for community ownership.
          </h3>
          <p
            class="display mt-8 text-[clamp(4.5rem,13vw,9rem)] leading-none text-panda"
          >
            80%
          </p>
          <p class="mt-4 max-w-sm text-pretty leading-relaxed text-ink-70">
            At genesis, 80% of PANDA was allocated to the DAO Treasury, placing
            the majority of the token supply under community governance.
          </p>
        </Reveal>
      </div>

      <div class="lg:col-span-7">
        <Reveal delay={80}>
          <AllocationChart />

          <div class="mt-8 border-l-2 border-ink/20 pl-5">
            <p class="eyebrow">Historical allocation</p>
            <p class="mt-3 text-pretty text-sm leading-relaxed text-ink-70">
              This section describes the genesis allocation of PANDA. Treasury
              distributions, governance rewards, burns, transfers, and other
              changes after launch are reflected in the on-chain state.
            </p>
          </div>

          <div class="mt-8">
            <LinkOut
              href={LINKS.snsTransactions}
              label="Explore On-chain Tokenomics"
            />
          </div>
        </Reveal>
      </div>
    </div>
  </div>

  <!-- What PANDA does -->
  <div class="mt-16 border-t border-ink/15 pt-14 md:mt-24 md:pt-20">
    <Reveal>
      <p class="eyebrow">What PANDA Does</p>
    </Reveal>
    <Reveal>
      <div class="mt-10 grid grid-cols-1 gap-px bg-ink/15 md:grid-cols-3">
        {#each PANDA_ROLES as role (role.name)}
          <div class="h-full bg-white p-6 md:p-8">
            <p class="font-mono text-xs tabular-nums text-ink-30">
              {role.index}
            </p>
            <h4 class="display mt-6 text-3xl">{role.name}</h4>
            <p class="mt-4 text-pretty leading-relaxed text-ink-70">
              {role.body}
            </p>
          </div>
        {/each}
      </div>
    </Reveal>
  </div>

  <!-- Trade (deliberately the smallest thing in this section) -->
  <Reveal>
    <div
      class="mb-16 mt-10 flex flex-col gap-4 border-t border-ink/10 pt-6 md:mb-24 md:flex-row md:items-center md:justify-between"
    >
      <p class="eyebrow">Trade PANDA</p>
      <div class="flex flex-wrap items-center gap-x-6 gap-y-3">
        <LinkOut href={LINKS.icpswap} label="ICPSwap" />
        <LinkOut href={LINKS.bridge} label="Bridge to BNB" />
        <a
          class="font-mono text-xs text-ink-70 underline decoration-ink/20 underline-offset-4 transition-colors hover:text-ink"
          href={LINKS.bscscan}
          target="_blank"
          rel="noreferrer"
        >
          BNB Chain: {PANDA_BNB_CONTRACT.slice(0, 6)}…{PANDA_BNB_CONTRACT.slice(
            -4
          )}
        </a>
      </div>
    </div>
  </Reveal>
</SectionShell>
