<script lang="ts">
  import Field from '../../components/page/Field.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { assetIdentity, assetKey, type Asset } from './model'
  import { w } from './text'
  let { value = $bindable(), metadata }: { value: any; metadata: Asset[] } = $props()
  let type = $state('ICP'),
    chain = $state('1'),
    contract = $state('')
  const options = $derived(
    value.spendable_assets.map((key: string) => ({
      value: key,
      label: metadata.find((a) => assetKey(a) === key) ? assetIdentity(metadata.find((a) => assetKey(a) === key)!) : key
    }))
  )
  function addAsset() {
    const key =
      type === 'ICP'
        ? 'icp'
        : type === 'BTC'
          ? 'btc'
          : type === 'SOL'
            ? 'sol'
            : type === 'ICRC'
              ? `icrc:${contract.trim()}`
              : type === 'SPL'
                ? `sol:${contract.trim()}`
                : `evm:${chain}${type === 'ERC20' ? `:${contract.trim()}` : ''}`
    if (!value.spendable_assets.includes(key)) value.spendable_assets = [...value.spendable_assets, key]
    if (['ETH', 'ERC20'].includes(type) && !value.evm_chains.includes(Number(chain)))
      value.evm_chains = [...value.evm_chains, Number(chain)]
    contract = ''
  }
</script>

<div class="grid gap-5" data-testid="wallet-policy-editor">
  <div class="grid gap-3 sm:grid-cols-2">
    {#each ['approver_threshold', 'admin_threshold', 'daily_signatures'] as field}
      <Field label={w(field)}><input type="number" min="1" bind:value={value[field]} /></Field>
    {/each}
    {#each ['address_book_cooldown_ms', 'proposal_ttl_ms', 'unfreeze_delay_ms'] as field}
      <Field label={w(field)}><input type="number" min="0" step="1000" bind:value={value[field]} /></Field>
    {/each}
  </div>
  <div class="flex flex-wrap gap-4"
    ><Checkbox bind:checked={value.public}>{w('public')}</Checkbox><Checkbox bind:checked={value.hardware_cosigners}
      >{w('hardware_cosigners')}</Checkbox
    ></div
  >
  <section class="border-line grid gap-2 rounded-xl border p-3">
    <h4 class="text-[13px] font-semibold">{w('spendable_assets')}</h4>
    {#each options as a (a.value)}<div class="flex items-center gap-2 text-[12px]"
        ><span class="min-w-0 flex-1 break-all">{a.label}</span><button
          class={buttonClass('ghost', 'xs')}
          onclick={() => (value.spendable_assets = value.spendable_assets.filter((k: string) => k !== a.value))}>{w('remove')}</button
        ></div
      >{/each}
    <div class="flex flex-wrap items-end gap-2">
      <Field label={w('asset')}
        ><Select
          bind:value={type}
          options={['ICP', 'BTC', 'ETH', 'SOL', 'ICRC', 'ERC20', 'SPL'].map((v) => ({ value: v, label: v }))}
        /></Field
      >
      {#if ['ETH', 'ERC20'].includes(type)}<Field label={w('chain_id')}><input type="number" min="1" bind:value={chain} /></Field>{/if}
      {#if ['ICRC', 'ERC20', 'SPL'].includes(type)}<Field label="Ledger / token / mint"
          ><input bind:value={contract} class="font-mono" dir="ltr" /></Field
        >{/if}
      <button
        class={buttonClass('secondary', 'sm')}
        disabled={['ICRC', 'ERC20', 'SPL'].includes(type) && !contract.trim()}
        onclick={addAsset} aria-label={w('addAsset')}>+ {w('asset')}</button
      >
    </div>
  </section>
  <Field label={w('evm_chains')}
    ><input
      value={value.evm_chains.join(', ')}
      onchange={(e) =>
        (value.evm_chains = e.currentTarget.value
          .split(',')
          .map((s) => Number(s.trim()))
          .filter(Boolean))}
      dir="ltr"
    /></Field
  >
  <Field label={w('call_targets')}
    ><textarea
      rows="2"
      value={value.call_targets.join('\n')}
      onchange={(e) => (value.call_targets = e.currentTarget.value.split(/\s+/).filter(Boolean))}
      dir="ltr"
      class="font-mono"></textarea></Field
  >
  <section class="grid gap-2">
    <h4 class="text-[13px] font-semibold">{w('tiers')}</h4>
    {#each value.tiers as row, i (i)}
      <div class="border-line grid gap-2 rounded-xl border p-3 sm:grid-cols-2">
        <Field label={w('asset')}><Select bind:value={row.asset} {options} /></Field>
        <Field label={w('up_to')}><input bind:value={row.up_to} inputmode="numeric" dir="ltr" /></Field>
        <Field label={w('approvals')}><input type="number" min="1" bind:value={row.approvals} /></Field>
        <Field label={w('delay_ms')}><input type="number" min="0" bind:value={row.delay_ms} /></Field>
        <Checkbox bind:checked={row.book_only}>{w('book_only')}</Checkbox>
        <button
          class={buttonClass('ghost', 'xs', 'justify-self-end')}
          onclick={() => (value.tiers = value.tiers.filter((_: any, n: number) => n !== i))}>{w('remove')}</button
        >
      </div>
    {/each}
    <button
      class={buttonClass('secondary', 'xs', 'justify-self-start')}
      disabled={!options.length}
      onclick={() =>
        (value.tiers = [
          ...value.tiers,
          { asset: options[0].value, up_to: '0', approvals: value.approver_threshold, delay_ms: 0, book_only: false }
        ])}>+ {w('tiers')}</button
    >
  </section>
  <section class="grid gap-2">
    <h4 class="text-[13px] font-semibold">{w('daily_limits')}</h4>
    {#each value.daily_limits as row, i (i)}
      <div class="flex flex-wrap items-end gap-2">
        <Field label={w('asset')}><Select bind:value={row.asset} {options} /></Field>
        <Field label={`${w('amount')} (${w('atomic')})`}><input bind:value={row.amount} inputmode="numeric" dir="ltr" /></Field>
        <button
          class={buttonClass('ghost', 'xs')}
          onclick={() => (value.daily_limits = value.daily_limits.filter((_: any, n: number) => n !== i))}>{w('remove')}</button
        >
      </div>
    {/each}
    <button
      class={buttonClass('secondary', 'xs', 'justify-self-start')}
      disabled={!options.length}
      onclick={() => (value.daily_limits = [...value.daily_limits, { asset: options[0].value, amount: '0' }])}>+ {w('daily_limits')}</button
    >
  </section>
  <section class="border-line grid gap-3 rounded-xl border p-3">
    <Checkbox
      checked={!!value.refuel}
      onCheckedChange={(on) => (value.refuel = on ? { from: 0, min_cycles: '1000000000000', icp_e8s: '100000000', max_per_day: 1 } : null)}
      >{w('autoRefuel')}</Checkbox
    >
    {#if value.refuel}<div class="grid gap-3 sm:grid-cols-2"
        >{#each ['from', 'min_cycles', 'icp_e8s', 'max_per_day'] as field}<Field label={w(field)}
            ><input
              type={['from', 'max_per_day'].includes(field) ? 'number' : 'text'}
              min="0"
              bind:value={value.refuel[field]}
              inputmode="numeric"
              dir="ltr"
            /></Field
          >{/each}</div
      >{/if}
  </section>
</div>
