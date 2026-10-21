<script lang="ts">
  import { onMount } from 'svelte'
  import Section from '../../components/page/Section.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Select from '../../components/Select.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import { buttonClass } from '../../components/ui'
  import { call, when } from '../../rpc'
  import { t } from '../../i18n/index.svelte'
  import { walletError as errorText } from './errors'
  import { cyclesText, utf8Length, type Wallet } from './model'
  import { w } from './text'
  let { wallet, onManage, onUpdated, onProposed }: { wallet: Wallet; onManage: (kind: string, target?: any) => void; onUpdated: () => void; onProposed: (p: any) => void } = $props()
  let versions = $state<any>(null),
    health = $state<any>(null),
    rpc = $state<any>(null),
    checks = $state<any>(null),
    channels = $state<any[]>([]),
    channel = $state('')
  let keys = $state<any[]>([]),
    keyLabel = $state(''),
    hardware = $state(true),
    busy = $state(false),
    error = $state(''),
    notice = $state(''),
    lockConfirmed = $state(false)
  let alive = true
  async function load() {
    try {
      const results = await Promise.allSettled([
        call('wallet/versions', { wallet: wallet.wallet }),
        call('wallet/rpc'),
        call('wallet/cosigners'),
        call('channel/list'),
        call('wallet/channel', { wallet: wallet.wallet })
      ])
      if (!alive) return
      const [v, r, k, c, b] = results
      if (v.status === 'fulfilled') versions = v.value
      else error = errorText(v.reason)
      if (r.status === 'fulfilled') rpc = r.value
      else error = errorText(r.reason)
      if (k.status === 'fulfilled') keys = k.value
      if (c.status === 'fulfilled') channels = c.value
      if (b.status === 'fulfilled') channel = b.value.channel ?? wallet.channel_hint ?? ''
    } catch (e) {
      if (alive) error = errorText(e)
    }
  }
  async function run(method: string, params: any = {}) {
    busy = true
    error = ''
    notice = ''
    try {
      const result: any = await call(method, { wallet: wallet.wallet, ...params })
      if (!alive) return
      if (method === 'wallet/change') {
        onProposed(result)
        return
      }
      if (method === 'wallet/health') health = result
      else if (method === 'wallet/rpc_check') checks = result
      else {
        notice = w('saved')
        onUpdated()
        await load()
      }
      if (result.notification_error) notice = w('notificationFailed')
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }
  const active = $derived(wallet.me?.status === 'Active')
  const mayPropose = $derived(active && !['Viewer', 'Recovery'].includes(wallet.me?.role ?? ''))
  onMount(() => {
    void load()
    return () => {
      alive = false
    }
  })
</script>

<div class="grid gap-5">
  {#if error}<Notice tone="danger">{error}<button class={buttonClass('ghost', 'xs')} onclick={load}>{w('retry')}</button></Notice>{/if}
  {#if notice}<Notice>{notice}</Notice>{/if}
  <Section title={w('cycles')} icon="coins">
    <div class="grid gap-3 sm:grid-cols-3">
      <div
        ><p class="text-faint text-[12px]">{w('cycles')}</p><b class="text-[22px] whitespace-nowrap tabular-nums" data-testid="wallet-health-cycles"
          >{cyclesText(health?.cycles ?? wallet.stats?.cycles ?? '0')} T</b
        ></div
      >
      <div
        ><p class="text-faint text-[12px]">{w('daily_signatures')}</p><b
          >{wallet.stats?.signatures_today ?? '—'} / {wallet.policy.daily_signatures}</b
        ></div
      >
      {#if health}<div
          ><p class="text-faint text-[12px]">{w('idleDays')}</p><b>{health.idle_days ?? '—'}</b><small class="text-faint block"
            >{when(health.read_at)}</small
          ></div
        >{/if}
    </div>
    {#if wallet.policy.refuel && BigInt(wallet.stats?.cycles ?? '0') < BigInt(wallet.policy.refuel.min_cycles)}<Notice tone="warn"
        >{w('lowCycles')}</Notice
      >{/if}
    <div class="mt-4 flex flex-wrap gap-2"
      ><button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/health')}>{w('health')}</button
      >{#if mayPropose}<button class={buttonClass('secondary', 'sm')} onclick={() => onManage('refuel')}>{w('refuel')}</button
        >{/if}{#if wallet.policy.refuel}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/refuel')}
          >{w('autoRefuel')}</button
        >{/if}</div
    >
  </Section>
  <Section title={w('cosigners')} icon="key">
    <p class="text-muted mb-3 text-[12px]">{w('cosignSetupHint')}</p>
    <div class="grid gap-2"
      >{#each wallet.cosigners as c (c.id)}<div class="border-line flex flex-wrap items-center gap-2 rounded-lg border p-3"
          ><b class="text-[13px]">#{c.id} · {c.label}</b><Pill tone={c.hardware ? 'ok' : 'warn'}
            >{c.hardware ? w('hardware') : w('softwareWarning')}</Pill
          ><span class="min-w-0 flex-1 font-mono text-[11px] break-all">{c.member} · {c.public_key}</span>{#if mayPropose}<button
              class={buttonClass('ghost', 'xs')}
              onclick={() => onManage('remove_cosigner', c)}>{w('remove')}</button
            >{/if}</div
        >{:else}<p class="text-faint text-[12px]">{w('none')}</p>{/each}</div
    >
    <h4 class="mt-5 mb-2 text-[13px] font-semibold">{w('localKeys')}</h4>
    {#each keys as k (k.label)}<div class="bg-surface-2 mb-2 flex flex-wrap items-center gap-2 rounded-lg p-3"
        ><b class="text-[13px]">{k.label}</b><Pill tone={k.hardware ? 'ok' : 'warn'}>{k.backend}</Pill><span
          class="min-w-0 flex-1 font-mono text-[11px] break-all">{k.public_key}</span
        >{#if mayPropose && !wallet.cosigners.some((c) => c.public_key === k.public_key)}<button
            class={buttonClass('secondary', 'xs')}
            disabled={busy}
            onclick={() => run('wallet/change', { change: { kind: 'cosigner', label: k.label } })}>{w('register')}</button
          >{/if}</div
      >{/each}
    <div class="mt-3 grid gap-3"
      ><Field label={w('label')}><input bind:value={keyLabel} maxlength="64" /></Field><Checkbox bind:checked={hardware}
        >{w('hardwareOnly')}</Checkbox
      >{#if !hardware}<Notice tone="warn">{w('softwareWarning')}</Notice>{/if}<button
        class={buttonClass('secondary', 'sm', 'justify-self-start')}
        disabled={busy || !keyLabel.trim() || utf8Length(keyLabel.trim())>64}
        onclick={() => run('wallet/cosigner_new', { label: keyLabel.trim(), hardware })}>{t('twNewCosigner')}</button
      ></div
    >
  </Section>
  <Section title={w('versions')} icon="settings">
    {#if versions}
      <p class="mb-3 text-[12px]"
        >{w('version')}: {versions.record?.version ?? '—'} · {versions.record?.locked ? w('lock_code') : wallet.code}</p
      >
      {#each versions.versions as v (v.version)}<div class="border-line flex flex-wrap items-center gap-2 border-t py-3"
          ><b>v{v.version}</b><div class="min-w-0 flex-1"
            ><p class="text-[12px]">{v.notes}</p><p class="text-faint font-mono text-[11px] break-all">{v.module_hash}</p></div
          >{#if mayPropose && !versions.record?.locked && !v.retired && v.version !== versions.record?.version}<button
              class={buttonClass('secondary', 'sm')}
              disabled={busy}
              onclick={() => run('wallet/change', { change: { kind: 'upgrade', version: v.version, module_hash: v.module_hash } })}
              >{w('upgrade')}</button
            >{/if}</div
        >{/each}
      {#if wallet.code_acceptance?.Upgrade}<button
          class={buttonClass('primary', 'sm')}
          disabled={busy}
          onclick={() => run('wallet/upgrade')}>{w('applyUpgrade')}</button
        >{/if}
      {#if !versions.record?.locked && mayPropose}<div class="border-line mt-3 border-t pt-3"
          ><button class={buttonClass('ghost', 'sm')} onclick={() => onManage('lock_code')}>{w('lockCode')}</button></div
        >{/if}
      {#if wallet.code_acceptance === 'Lock'}<Notice tone="danger"
          >{w('lockHint')}<div class="mt-3"
            ><Checkbox bind:checked={lockConfirmed}>{w('applyLock')}</Checkbox><button
              class={buttonClass('danger', 'sm', 'mt-3')}
              disabled={busy || !lockConfirmed}
              onclick={() => run('wallet/lock_code')}>{w('applyLock')}</button
            ></div
          ></Notice
        >{/if}
    {:else}<button class={buttonClass('secondary', 'sm')} onclick={load}>{w('retry')}</button>{/if}
  </Section>
  <Section title={w('channel')} description={w('channelHint')} icon="channels">
    <div class="flex flex-wrap gap-2"
      ><Select
        bind:value={channel}
        label={w('channel')}
        options={[{ value: '', label: w('none') }, ...channels.map((c) => ({ value: c.id, label: c.name }))]}
      /><button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/channel', { channel })}
        >{w('save')}</button
      ><button class={buttonClass('secondary', 'sm')} disabled={busy || !channel} onclick={() => run('wallet/notify')}>{w('notify')}</button
      ></div
    >
  </Section>
  <Section title={w('rpc')} description={w('rpcHint')} icon="link">
    {#if rpc}
      <div class="grid gap-3">
        <Field label="Bitcoin · Esplora"><input bind:value={rpc.bitcoin_api} type="url" dir="ltr" /></Field>
        <Field label="Solana · RPC"><input bind:value={rpc.solana_rpc} type="url" dir="ltr" /></Field>
        {#each rpc.evm_rpc as row, i (i)}<div class="grid gap-2 sm:grid-cols-[7rem_minmax(0,1fr)_auto]"
            ><Field label={w('chain_id')}><input type="number" min="1" bind:value={row[0]} dir="ltr" /></Field><Field label="EVM · RPC"
              ><input bind:value={row[1]} type="url" dir="ltr" /></Field
            ><button
              class={buttonClass('ghost', 'xs', 'self-end')}
              onclick={() => (rpc.evm_rpc = rpc.evm_rpc.filter((_: any, n: number) => n !== i))}>{w('remove')}</button
            ></div
          >{/each}
        <button class={buttonClass('ghost', 'sm', 'justify-self-start')} onclick={() => (rpc.evm_rpc = [...rpc.evm_rpc, [1, '']])}
          >+ EVM</button
        >
        <div class="flex flex-wrap gap-2"
          ><button class={buttonClass('primary', 'sm')} disabled={busy} onclick={() => run('wallet/rpc_set', { settings: rpc })}
            >{w('save')}</button
          ><button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/rpc_check')}
            >{w('checkConnections')}</button
          ></div
        >
        {#if checks}{#each checks.endpoints as e, i (i)}<p class="text-[12px] break-all" class:text-danger={e.result.Err}
              >{e.chain} · {e.source} · {e.result.Ok ?? e.result.Err}</p
            >{/each}<p class="text-faint text-[11px]">{when(checks.read_at)}</p>{/if}
      </div>
    {:else}<button class={buttonClass('secondary', 'sm')} onclick={load}>{w('retry')}</button>{/if}
  </Section>
</div>
