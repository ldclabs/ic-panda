<script lang="ts">
  import { TokenDisplay, type TokenInfo } from '$lib/utils/token'
  import Loading from './Loading.svelte'

  let selfClass: string = ''

  export { selfClass as class }
  export let token: TokenInfo
  export let amount: Promise<bigint>

  $: tokenDisplay = async () => new TokenDisplay(token, await amount)
</script>

<div class={selfClass}>
  {#await tokenDisplay()}
    <span><Loading /></span>
    <span>{token.symbol}</span>
  {:then val}
    {@const amountString = val.display()}
    <span class="text-right font-medium" title={amountString}>
      {val.short()}
    </span>
    <span>{token.symbol}</span>
  {:catch}
    <span class="text-right">N/A</span>
    <span>{token.symbol}</span>
  {/await}
</div>
