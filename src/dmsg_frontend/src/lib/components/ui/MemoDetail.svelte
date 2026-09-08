<script lang="ts">
  import { t } from '$lib/i18n'
  import { mapToObj } from '$lib/utils/fetcher'
  import { decodeCBOR } from '@ldclabs/cose-ts/utils'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    memo: Uint8Array | number[] | null
  }

  let { memo }: Props = $props()

  const detail: { message: string; link: string } | null = memo
    ? mapToObj(decodeCBOR(memo as Uint8Array))
    : null
</script>

{#if detail}
  <div class="mt-2 pl-8 text-sm">
    <p><b>{$t('Message:')}</b></p>
    <p class="text-neutral-500">
      {detail.message || '-'}
    </p>
    {#if detail.link}
      <p><b>{$t('Link:')}</b></p>
      <a
        class="block w-full truncate text-neutral-500 underline"
        href={detail.link}
        target="_blank">{detail.link}</a
      >
    {/if}
  </div>
{/if}
