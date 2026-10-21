<script lang="ts">
  import type { Wallet } from './model'
  import { w } from './text'
  let { policy, wallet, address, title }: { policy: any; wallet?: Wallet; address?: number; title?: string } = $props()
  const source = $derived(wallet?.addresses.find((a) => a.id === address))
</script>

<section class="border-line grid content-start gap-2 rounded-lg border p-3 text-[12px]" data-testid="wallet-cosign-policy">
  {#if title}<h4 class="font-semibold">{title}</h4>{/if}
  {#if address !== undefined}
    <p>#{address} · {source?.label || source?.chain || w('address')}</p>
    {#if source?.text}<p class="font-mono break-all select-all" dir="ltr">{source.text}</p>{/if}
  {/if}
  {#if policy}
    <p class="font-semibold">{w('threshold')}: {policy.threshold} / {policy.cosigners.length}</p>
    {#each policy.cosigners as id (id)}
      {@const key = wallet?.cosigners.find((c) => c.id === id)}
      <div class="grid gap-1">
        <p>#{id}{#if key} · {key.label} · {key.hardware ? w('hardware') : w('softwareKey')}{/if}</p>
        {#if key}<p class="text-muted break-all">{key.member}</p><p class="font-mono text-[11px] break-all" dir="ltr">{key.public_key}</p>{/if}
      </div>
    {/each}
  {:else}<p class="font-semibold">{w(source?.control === 'taproot_2of2' ? 'nativeCosign' : 'cosignDisabled')}</p>{/if}
</section>
