<script lang="ts">
  import { mapToObj } from '$lib/utils/fetcher'
  import { decodeCBOR } from '@ldclabs/cose-ts/utils'

  // Props
  /** Exposes parent props to this component. */
  export let memo: Uint8Array | number[] | null

  const detail: { message: string; link: string } | null = memo
    ? mapToObj(decodeCBOR(memo as Uint8Array))
    : null
</script>

{#if detail}
  <div class="mt-2 pl-8 text-sm">
    <p><b>Message:</b></p>
    <p class="text-neutral-600">
      {detail.message || '-'}
    </p>
    {#if detail.link}
      <p><b>Link:</b></p>
      <a
        class="block w-full truncate text-neutral-600 underline"
        href={detail.link}
        target="_blank">{detail.link}</a
      >
    {/if}
  </div>
{/if}
