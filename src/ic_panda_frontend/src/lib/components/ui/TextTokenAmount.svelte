<script lang="ts">
  import { popup } from '@skeletonlabs/skeleton'
  import { type Token } from '@dfinity/utils'
  import { TokenAmount, formatToken } from '$lib/utils/token'
  import Loading from './Loading.svelte'

  export let selfClass: string
  export let token: Token
  export let amount: Promise<bigint>

  $: tokenDisplay = async () =>
    formatToken(TokenAmount.fromUlps({ amount: await amount, token }))
</script>

<div class={selfClass}>
  {#await tokenDisplay()}
    <Loading />
  {:then val}
    <span
      class="text-right"
      use:popup={{
        event: 'hover',
        target: 'TAD-' + val.detail
      }}
    >
      {val.display}
    </span>
    <span>{token.symbol}</span>
    <div
      class="card bg-surface-800 px-2 py-1 text-white"
      data-popup="TAD-{val.detail}"
    >
      <p>{val.detail}</p>
      <div class="arrow bg-surface-800" />
    </div>
  {:catch}
    <span class="text-right">N/A</span>
    <span>{token.symbol}</span>
  {/await}
</div>
