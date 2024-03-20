<script lang="ts">
  import { TokenAmount, formatToken, type TokenInfo } from '$lib/utils/token'
  import { popup } from '@skeletonlabs/skeleton'
  import Loading from './Loading.svelte'

  let selfClass: string = ''

  export { selfClass as class }
  export let token: TokenInfo
  export let amount: Promise<bigint>

  $: tokenDisplay = async () =>
    formatToken(TokenAmount.fromUlps({ amount: await amount, token }))
</script>

<div class={selfClass}>
  {#await tokenDisplay()}
    <span><Loading /></span>
    <span>{token.symbol}</span>
  {:then val}
    <span
      class="text-right font-medium"
      use:popup={{
        event: 'hover',
        target: 'TAD-' + val.full
      }}
    >
      {val.display}
    </span>
    <span>{token.symbol}</span>
    <div
      class="card bg-surface-800 px-2 py-0 text-white"
      data-popup="TAD-{val.full}"
    >
      <p>{val.full}</p>
      <div class="arrow bg-surface-800" />
    </div>
  {:catch}
    <span class="text-right">N/A</span>
    <span>{token.symbol}</span>
  {/await}
</div>
