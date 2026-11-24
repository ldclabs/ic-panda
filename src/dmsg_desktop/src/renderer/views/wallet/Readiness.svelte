<script lang="ts">
  import { teamReadiness, type Wallet } from './model'
  import { w } from './text'
  let { wallet }: { wallet: Wallet } = $props()
</script>

<section class="border-line grid gap-3 rounded-xl border p-4" data-testid="wallet-team-readiness">
  <h3 class="font-semibold text-title-3">{w('teamReadiness')}</h3>
  {#each teamReadiness(wallet) as row (row.key)}
    <p class="text-body" class:text-warn={row.warn}>{w(row.key)} {row.count ?? ''}</p>
  {/each}
  <p class="text-muted text-callout">{w('oneMemberOneVote')}</p>
</section>
