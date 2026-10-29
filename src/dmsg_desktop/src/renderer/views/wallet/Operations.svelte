<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Section from '../../components/page/Section.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Select from '../../components/Select.svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Confirm from '../../components/Confirm.svelte'
  import { buttonClass } from '../../components/ui'
  import { CallError, call, single, when } from '../../rpc'
  import { t } from '../../i18n/index.svelte'
  import { walletError as errorText } from './errors'
  import { cyclesText, decimal, utf8Length, type Wallet } from './model'
  import { w } from './text'
  let {
    wallet,
    onManage,
    onUpdated,
    onProposed
  }: { wallet: Wallet; onManage: (kind: string, target?: any) => void; onUpdated: () => void; onProposed: (p: any) => void } = $props()
  let versions = $state<any>(null),
    health = $state<any>(null),
    rpc = $state<any>(null),
    checks = $state<any>(null),
    channels = $state<any[]>([]),
    channel = $state('')
  let keys = $state<any[]>([]),
    keyLabel = $state(''),
    // Without a Secure Enclave the person decides on a software key.
    softwareKey = $state(false),
    busy = $state(false),
    error = $state(''),
    notice = $state(''),
    lockConfirmed = $state(false)
  let setupStep = $state(0),
    tested = $state<Record<string, boolean>>({}),
    rollback = $state<any>(null),
    balance = $state<any>(null)
  const activeStep = $derived(
    setupStep ||
      (!keys.length
        ? 1
        : !wallet.cosigners.some((c) => keys.some((k) => k.public_key === c.public_key))
          ? 2
          : !wallet.addresses.some((a) => a.cosign)
            ? 3
            : 4)
  )
  const latestVersion = $derived(Math.max(0, ...(versions?.versions ?? []).filter((v: any) => !v.retired).map((v: any) => v.version)))
  const acceptedUpgrade = $derived(wallet.code_acceptance?.Upgrade)
  const pendingUpgrade = $derived(acceptedUpgrade && acceptedUpgrade.version !== versions?.record?.version)
  let codePending = $state(false), codeError = $state(''), installedVersion = $state<number | null>(null)
  let versionRead = 0
  let alive = true
  let errors = $state<Record<string, string>>({})
  function reader(key: string, method: string, apply: (value: any) => void) {
    return single(async () => {
      if (!alive) return
      try {
        const value = await call(method, { wallet: wallet.wallet })
        if (alive) {
          apply(value)
          errors[key] = ''
        }
      } catch (e) {
        if (alive) errors[key] = errorText(e)
      }
    })
  }
  const readHealth = reader('health', 'wallet/health', (value) => (health = value))
  const readVersions = single(async () => {
    if (!alive) return
    const read = ++versionRead
    try {
      const value = await call('wallet/versions', { wallet: wallet.wallet })
      if (alive && read === versionRead) { versions = value; errors.versions = '' }
    } catch (e) {
      if (alive && read === versionRead) errors.versions = errorText(e)
    }
  })
  const readRpc = reader('rpc', 'wallet/rpc', (value) => (rpc = value))
  const readKeys = reader('keys', 'wallet/cosigners', (value) => (keys = value))
  const readChannels = reader('channels', 'channel/list', (value) => (channels = value))
  const readBinding = reader('binding', 'wallet/channel', (value) => (channel = value.channel ?? wallet.channel_hint ?? ''))
  function load() {
    void readVersions()
    void readRpc()
    void readKeys()
    void readChannels()
    void readBinding()
  }
  async function installUpgrade() {
    if (busy || !pendingUpgrade) return
    busy = true
    codePending = true
    codeError = ''
    installedVersion = null
    try {
      const result: any = await call('wallet/upgrade', { wallet: wallet.wallet })
      if (!alive) return
      // The hub's completed install takes precedence over an older read in flight.
      ++versionRead
      versions = { ...versions, record: result.record }
      errors.versions = ''
      installedVersion = result.record.version
      onUpdated()
    } catch (e) {
      if (!alive) return
      codeError = errorText(e)
      void readVersions()
      onUpdated()
    } finally {
      busy = false
      codePending = false
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
      if (method === 'wallet/cosigner_test') {
        tested[params.label] = result.verified === true
        notice = w('keyTestPassed')
      } else if (method === 'wallet/health') health = result
      else if (method === 'wallet/balances') balance = result
      else if (method === 'wallet/rpc_check') checks = result
      else {
        notice = w('saved')
        onUpdated()
        if (method === 'wallet/rpc_set') void readRpc()
        else if (method.startsWith('wallet/cosigner')) {
          setupStep = 2
          void readKeys()
        } else if (method === 'wallet/channel') void readBinding()
        else load()
      }
      if (result.notification_error) notice = w('notificationFailed')
    } catch (e) {
      if (!alive) return
      if (method === 'wallet/cosigner_new' && params.hardware && e instanceof CallError && e.code === 'UNAVAILABLE') softwareKey = true
      else error = errorText(e)
    } finally {
      busy = false
    }
  }
  const active = $derived(wallet.me?.status === 'Active')
  const mayPropose = $derived(active && !['Viewer', 'Recovery'].includes(wallet.me?.role ?? ''))
  $effect(() => {
    wallet
    untrack(() => {
      void readVersions()
      void readKeys()
    })
  })
  onMount(() => {
    void load()
    void readHealth()
    return () => {
      alive = false
    }
  })
</script>

<div class="grid gap-5">
  {#each Object.entries(errors).filter(([key, value]) => key !== 'versions' && value).map(([, value]) => value) as failure}<Notice failure={failure} tone="danger"
      >{failure}<button class={buttonClass('ghost', 'xs')} onclick={load}>{w('retry')}</button></Notice
    >{/each}
  {#if error}<Notice failure={error} tone="danger">{error}<button class={buttonClass('ghost', 'xs')} onclick={load}>{w('retry')}</button></Notice>{/if}
  {#if notice}<Notice>{notice}</Notice>{/if}
  <Section title={w('runningHealth')} icon="coins">
    <div class="grid gap-3 sm:grid-cols-3">
      <div
        ><p class="text-faint text-[12px]">{w('cycles')}</p><b
          class="text-[22px] whitespace-nowrap tabular-nums"
          data-testid="wallet-health-cycles">{cyclesText(health?.cycles ?? wallet.stats?.cycles ?? '0')} T</b
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
    <p class="text-muted mt-3 text-[12px]">{w(health ? 'idleEstimateHint' : 'healthUnknown')}</p>
    {#if wallet.policy.refuel}<p class="mt-3 text-[13px]"
        >{w('dailyTopupMax')}: {decimal((BigInt(wallet.policy.refuel.icp_e8s) * BigInt(wallet.policy.refuel.max_per_day)).toString(), 8)} ICP</p
      ><p class="text-muted mt-2 text-[12px]">{w('topupRuleHint')}</p>{/if}
    {#if wallet.policy.refuel && BigInt(wallet.stats?.cycles ?? '0') < BigInt(wallet.policy.refuel.min_cycles)}<Notice tone="warn"
        >{w('lowCycles')}</Notice
      >{/if}
    <div class="mt-4 flex flex-wrap gap-2"
      ><button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/health')}>{w('health')}</button
      >{#if mayPropose}<button
          class={buttonClass('secondary', 'sm')}
          onclick={() => onManage('refuel', { amount: wallet.policy.refuel ? decimal(wallet.policy.refuel.icp_e8s, 8) : '1' })}
          >{w('refuel')}</button
        >{/if}{#if wallet.policy.refuel}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => run('wallet/refuel')}
          >{w('autoRefuel')}</button
        >{/if}</div
    >
  </Section>
  <Section title={w('cosigners')} icon="key">
    <p class="text-muted mb-3 text-[12px]">{w('cosignBoundary')}</p>
    <nav class="mb-4 grid gap-2 sm:grid-cols-2" aria-label={w('cosigners')}>
      {#each ['setupKeys', 'setupRegister', 'setupBind', 'setupTest'] as step, i}<button
          class={buttonClass(activeStep === i + 1 ? 'secondary' : 'ghost', 'sm')}
          aria-current={activeStep === i + 1 ? 'step' : undefined}
          onclick={() => (setupStep = i + 1)}>{w(step)}</button
        >{/each}
    </nav>
    {#if activeStep === 1}
      <Field label={w('label')}><input bind:value={keyLabel} maxlength="64" /></Field>
      <button
        class={buttonClass('primary', 'sm', 'mt-3')}
        disabled={busy || !keyLabel.trim() || utf8Length(keyLabel.trim()) > 64}
        onclick={() => run('wallet/cosigner_new', { label: keyLabel.trim(), hardware: true })}>{t('twNewCosigner')}</button
      >
    {:else if activeStep === 2}
      {#each keys as k (k.label)}<div class="border-line flex flex-wrap items-center gap-3 border-b py-3">
          <b>{k.label}</b><Pill>{k.backend}</Pill>
          {#if wallet.cosigners.some((c) => c.public_key === k.public_key)}<span class="text-ok text-[13px]">{w('cosigner_added')}</span>
          {:else if mayPropose}<button
              class={buttonClass('secondary', 'sm')}
              disabled={busy}
              onclick={() => run('wallet/change', { change: { kind: 'cosigner', label: k.label } })}>{w('register')}</button
            >{/if}
          <Collapsible class="w-full"
            >{#snippet title()}<span class="text-[12px]">{w('technical')}</span>{/snippet}<p class="font-mono text-[12px] break-all"
              >{k.public_key}</p
            ></Collapsible
          ></div
        >
      {:else}<Notice>{w('setupKeys')}</Notice>{/each}
    {:else if activeStep === 3}
      {#each wallet.addresses as a (a.id)}<div class="border-line flex flex-wrap items-center gap-3 border-b py-3">
          <span class="flex-1">{a.label || a.chain} · #{a.id}</span>
          <span class="text-[13px]"
            >{a.cosign ? `${w('cosign')} ${a.cosign.threshold} / ${a.cosign.cosigners.length}` : w('unprotected')}</span
          >
          {#if mayPropose && a.control !== 'taproot_2of2'}<button
              class={buttonClass('secondary', 'sm')}
              onclick={() => onManage('cosign', a)}>{w('bindCosign')}</button
            >{/if}</div
        >
      {/each}
    {:else}
      <p class="text-muted mb-3 text-[12px]">{w('keyTestHint')}</p>
      {#each keys as k (k.label)}<div class="border-line flex flex-wrap items-center gap-3 border-b py-3"
          ><b>{k.label}</b><Pill>{k.backend}</Pill><button
            class={buttonClass('secondary', 'sm')}
            disabled={busy}
            aria-label={`${w('testKey')} ${k.label}`}
            onclick={() => run('wallet/cosigner_test', { label: k.label })}>{w('testKey')}</button
          >{#if tested[k.label]}<span class="text-ok text-[13px]" role="status">{w('keyTestPassed')}</span>{/if}</div
        >{/each}
    {/if}
    <Collapsible class="mt-5"
      >{#snippet title()}<span class="text-[13px]">{w('cosigners')} · {w('technical')}</span>{/snippet}
      {#each wallet.cosigners as c (c.id)}<div class="border-line grid gap-2 border-b py-3"
          ><b>#{c.id} · {c.label}</b><p class="text-[12px] break-all">{c.member} · {c.public_key}</p>{#if mayPropose}<button
              class={buttonClass('ghost', 'sm', 'justify-self-start')}
              aria-label={`${w('remove')} ${c.label}`}
              onclick={() => onManage('remove_cosigner', c)}>{w('remove')}</button
            >{/if}</div
        >{/each}
    </Collapsible>
  </Section>
  <Section title={w('versions')} icon="settings">
    {#if errors.versions}<Notice failure={errors.versions} tone="danger">{errors.versions}<button class={buttonClass('ghost', 'xs')} onclick={readVersions}>{w('retry')}</button></Notice>{/if}
    {#if versions}
      <p class="mb-3 text-[12px]" data-testid="wallet-code-current"
        >{w('currentVersion')}: <bdi dir="ltr">v{versions.record?.version ?? '—'}</bdi>{#if versions.record?.locked} · {w('lock_code')}{:else if !pendingUpgrade} · {wallet.code_acceptance === 'Lock' ? w('applyLock') : w('noCodeChange')}{/if}</p
      >
      {#if pendingUpgrade}<p class="mb-3 text-[12px]" data-testid="wallet-code-accepted">{w('upgradeAccepted')}: <bdi dir="ltr">v{acceptedUpgrade.version}</bdi></p>{/if}
      {#each versions.versions as v (v.version)}<div class="border-line flex flex-wrap items-center gap-2 border-t py-3"
          ><b>v{v.version}</b>{#if v.version === versions.record?.version}<Pill>{w('currentVersion')}</Pill>{:else if v.retired}<Pill
              >{w('retiredVersion')}</Pill
            >{:else if v.version === latestVersion}<Pill>{w('recommendedVersion')}</Pill>{/if}<div class="min-w-0 flex-1"
            ><p class="text-[12px]">{v.notes || w('releaseNotesMissing')}</p><Collapsible
              >{#snippet title()}<span class="text-[12px]">{w('technical')}</span>{/snippet}<p class="font-mono text-[12px] break-all"
                >{v.module_hash}</p
              ></Collapsible
            ></div
          >{#if mayPropose && !versions.record?.locked && !v.retired && v.version !== versions.record?.version && v.version !== acceptedUpgrade?.version}<button
              class={buttonClass('secondary', 'sm')}
              disabled={busy}
              onclick={() =>
                v.version < versions.record.version
                  ? (rollback = v)
                  : run('wallet/change', { change: { kind: 'upgrade', version: v.version, module_hash: v.module_hash } })}
              >{w(v.version < versions.record.version ? 'rollback' : 'upgrade')}</button
            >{/if}</div
        >{/each}
      {#if rollback}<Notice tone="warn"
          >{w('rollbackHint')} · v{versions.record?.version} → v{rollback.version}<button
            class={buttonClass('danger', 'sm', 'mt-3')}
            disabled={busy}
            onclick={() =>
              run('wallet/change', { change: { kind: 'upgrade', version: rollback.version, module_hash: rollback.module_hash } })}
            >{w('rollback')}</button
          ></Notice
        >{/if}
      {#if codePending}<Notice class="mb-3" testid="wallet-upgrade-progress">{w('upgradeInstalling')}</Notice>{/if}
      {#if codeError}<Notice class="mb-3" tone="danger" testid="wallet-upgrade-error">{w('upgradeNotConfirmed')}<p class="mt-1">{codeError}</p><button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => { void readVersions(); onUpdated() }}>{t('refresh')}</button></Notice>{/if}
      {#if installedVersion !== null}<Notice class="mb-3" tone="ok" testid="wallet-upgrade-success">{w('upgradeInstalled')}: <bdi dir="ltr">v{installedVersion}</bdi></Notice>{/if}
      {#if pendingUpgrade}<button
          data-testid="wallet-upgrade-install"
          class={buttonClass('primary', 'sm')}
          disabled={busy}
          onclick={installUpgrade}>{w('applyUpgrade')}</button
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
    {:else}<p class="text-muted text-[12px]" role="status">{w(errors.versions ? 'retry' : 'loading')}</p>{/if}
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
<Confirm
  bind:open={softwareKey}
  title={w('softwareKeyTitle')}
  description={w('softwareKeyBody')}
  confirm={w('softwareKeyCreate')}
  icon="key"
  tone="warn"
  {busy}
  onConfirm={() => run('wallet/cosigner_new', { label: keyLabel.trim(), hardware: false })}
  testid="wallet-software-key"
/>
