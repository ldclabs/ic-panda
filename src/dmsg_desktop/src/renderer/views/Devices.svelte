<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { display, watchDisplays, type Mode } from '../display.svelte'
  import DisplayNotice from '../components/DisplayNotice.svelte'
  import Dialog from '../components/Dialog.svelte'
  import { sameSelection } from './security'
  import Checkbox from '../components/Checkbox.svelte'
  import Icon from '../components/Icon.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, errorText, onChange, single, when } from '../rpc'

  /** The account's devices as the user home certified them; changes are approval requests. */
  let { status }: { status: any } = $props()

  let alive = true
  onDestroy(() => { alive = false })
  let view = $state<any>(null)
  let editing = $state<string | null>(null)
  let chosen = $state<string[]>([])
  let error = $state('')
  let busy = $state(false)
  let loadError = $state('')
  let pending = $state<Record<string, string>>({})
  const target = $derived(view?.devices.find((device: any) => device.device_id === editing))
  const changed = $derived(!!target && !sameSelection(chosen, target.capabilities))
  const canSubmit = $derived(changed && !busy && !loadError && !target?.revoked_at && !pending[editing ?? ''])

  const CAPABILITIES = ['ContentSign', 'VaultUnlock', 'RootManage', 'FormalApprove', 'PaymentOffer', 'WalletApprove']

  const reload = single(async (mode: Mode = 'swr') => {
    if (!alive) return
    try {
      const next = await display('account/show', {}, mode)
      if (!alive) return
      if (target && sameSelection(chosen, target.capabilities)) {
        const updated = next.devices.find((device: any) => device.device_id === editing)
        if (updated) chosen = [...updated.capabilities]
      }
      view = next
      loadError = ''
      for (const [device, id] of Object.entries(pending)) {
        const result: any = await call('approval/show', { id }).catch(() => null)
        if (!alive) return
        if (['done', 'rejected', 'cancelled', 'expired', 'failed', 'elsewhere'].includes(result?.state)) delete pending[device]
      }
    } catch (e) {
      if (!alive) return
      loadError = errorText(e)
    }
  })

  /** The approval window shows the device and decides. */
  async function ask(method: string, params: { device: string; capabilities?: string[] }) {
    if (busy || loadError) return
    if (method === 'device/capabilities' && !canSubmit) return
    busy = true
    error = ''
    try {
      const id = pending[params.device] || (await call<any>(method, params)).id
      if (!alive) return
      pending[params.device] = id
      await bridge(() => window.dmsg.openApproval(id))
      if (alive) { editing = null; void reload() }
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    } finally {
      if (alive) busy = false
    }
  }

  function edit(device: any) {
    if (busy || pending[device.device_id]) return
    error = ''
    editing = editing === device.device_id ? null : device.device_id
    chosen = [...device.capabilities]
  }

  async function showPending(device: string) {
    const id = pending[device]
    if (busy || !id) return
    busy = true
    try { await bridge(() => window.dmsg.openApproval(id)) }
    catch (e) { if (alive) error = errorText(e) }
    finally { if (alive) busy = false }
  }

  onMount(() => {
    void reload()
    const off = onChange(['approval/changed', 'session/changed', 'sync/changed'], reload)
    const stop = watchDisplays(['account-summary'], (mode) => reload(mode))
    return () => { off(); stop() }
  })
</script>
<DisplayNotice methods={['account/show']} />

{#if loadError}<Notice failure={loadError} tone="danger">{loadError}
  {#snippet action()}<button class={buttonClass('secondary','xs')} disabled={busy} onclick={() => reload('fresh')}>{t('retry')}</button>{/snippet}
</Notice>{/if}
{#if error && !editing}<Notice failure={error} tone="danger">{error}</Notice>{/if}
{#if !view && !loadError}<p class="px-5 text-[13px] text-muted" role="status">{t('loading')}</p>{/if}
{#if view}
  <Section title={t('devices')} icon="device" flush testid="devices">
    <p class="flex flex-wrap gap-x-3 px-5 pb-3 text-[12px] text-faint" data-testid="security-snapshot">
      <span class="inline-flex items-center gap-1"><Icon name="verified" class="size-3.5 text-ok" />{t('snapshotChecked')}</span>
      <span>{t('accountVersion')} {view.account_version}</span>
      <span>{t('securityEpoch')} {view.security_epoch}</span>
      <span>{t('rootGeneration')} {view.root_generation}</span>
    </p>
    <ul class="divide-y divide-line">
      {#each view.devices as device (device.device_id)}
        <li class={cn('px-5 py-3.5', device.revoked_at && 'opacity-60')}>
          <div class="flex flex-wrap items-start gap-3">
            <span class={cn('grid size-9 shrink-0 place-items-center rounded-xl', device.current ? 'bg-pale text-forest' : 'bg-surface-2 text-faint')}>
              <Icon name="device" />
            </span>
            <div class="min-w-48 flex-1">
              <div class="flex flex-wrap items-center gap-1.5">
                <Id id={device.device_id} />
                {#if device.current}<Pill tone="forest">{t('thisDevice')}</Pill>{/if}
                <Pill>{device.role}</Pill>
                {#if device.revoked_at}<Pill tone="danger">{t('revokedAt')} {when(device.revoked_at)}</Pill>{/if}
              </div>
              <div class="mt-1.5 flex flex-wrap gap-1">
                {#each device.capabilities as capability (capability)}<Pill tone="ok" class="font-normal">{capability}</Pill>{:else}<span class="text-[12px] text-faint">-</span>{/each}
              </div>
              <p class="mt-1 text-[11.5px] text-faint">
                {t('addedAt')} {when(device.added_at)}{device.added_by ? ` · ${t('addedBy')} ${device.added_by.slice(0, 12)}` : ''}
              </p>
            </div>
            {#if !device.revoked_at && status?.state === 'unlocked'}
              <div class="flex shrink-0 gap-1.5">
                <button class={buttonClass('secondary', 'xs')} disabled={busy || !!loadError || !!pending[device.device_id]} aria-expanded={editing === device.device_id} onclick={() => edit(device)} data-testid="device-capabilities">
                  {t('capabilities')}
                </button>
                {#if !device.current}
                  <button class={buttonClass('danger', 'xs')} disabled={busy || !!loadError || !!pending[device.device_id]} data-testid="revoke-device" onclick={() => ask('device/revoke', { device: device.device_id })}>
                    {t('revoke')}
                  </button>
                {/if}
              </div>
            {/if}
          </div>
          {#if pending[device.device_id]}
            <Notice>{t('securityApprovalPending')}
              {#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => showPending(device.device_id)}>{t('securityViewApproval')}</button>{/snippet}
            </Notice>
          {/if}
        </li>
      {/each}
    </ul>
  </Section>
{/if}

<Dialog bind:open={() => editing !== null, (open) => { if (!open) { editing = null; error = '' } }} title={t('capabilities')} description={t('securityApprovalHint')} icon="device" dismissible={!busy} testid="device-editor">
  <div class="grid gap-3">
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    {#if target?.revoked_at}<Notice tone="warn">{t('revokedAt')} {when(target.revoked_at)}</Notice>{/if}
    {#if editing}<Id id={editing} />{/if}
    {#each CAPABILITIES as capability (capability)}
      <Checkbox checked={chosen.includes(capability)} disabled={busy || !!pending[editing ?? ''] || !!target?.revoked_at} onCheckedChange={(on) => (chosen = on ? [...chosen, capability] : chosen.filter(c => c !== capability))}>{capability}</Checkbox>
    {/each}
  </div>
  {#snippet footer()}
    {#if !changed}<span class="text-[12px] text-faint">{t('securityUnchanged')}</span>{/if}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => { editing = null; error = '' }}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={!canSubmit} data-testid="capabilities-submit" onclick={() => ask('device/capabilities', { device: editing!, capabilities: chosen })}>{t('submit')}</button>
  {/snippet}
</Dialog>
