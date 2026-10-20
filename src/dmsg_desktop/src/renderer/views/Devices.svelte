<script lang="ts">
  import { onMount } from 'svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import Icon from '../components/Icon.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, onChange, single, when } from '../rpc'

  /** The account's devices as the user home certified them; changes are approval requests. */
  let { status }: { status: any } = $props()

  let view = $state<any>(null)
  let editing = $state<string | null>(null)
  let chosen = $state<string[]>([])
  let error = $state('')
  let busy = $state(false)

  const CAPABILITIES = ['ContentSign', 'VaultUnlock', 'RootManage', 'FormalApprove', 'PaymentOffer', 'WalletApprove']

  const reload = single(async () => {
    try {
      view = await call('account/show')
    } catch (e) {
      view = null
      error = code(e) === 'AUTH_REQUIRED' ? '' : errorText(e)
    }
  })

  /** The approval window shows the device and decides. */
  async function ask(method: string, params: unknown) {
    busy = true
    error = ''
    try {
      const created: any = await call(method, params)
      editing = null
      await bridge(() => window.dmsg.openApproval(created.id))
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  function edit(device: any) {
    editing = editing === device.device_id ? null : device.device_id
    chosen = [...device.capabilities]
  }

  onMount(() => {
    void reload()
    return onChange(['approval/changed', 'session/changed', 'sync/changed'], reload)
  })
</script>

{#if error}<Notice tone="danger">{error}</Notice>{/if}
{#if view}
  <Section title={t('devices')} icon="device" flush testid="devices">
    <p class="flex flex-wrap gap-x-3 px-5 pb-3 text-[12px] text-faint" data-testid="security-snapshot">
      <span class="inline-flex items-center gap-1"><Icon name="verified" class="size-3.5 text-ok" />{t('snapshotChecked')}</span>
      <span>{t('accountVersion')} {view.account_version}</span>
      <span>{t('securityEpoch')} {view.security_epoch}</span>
      <span>{t('rootGeneration')} {view.root_generation}</span>
    </p>
    <ul class="divide-y divide-line border-t border-line">
      {#each view.devices as device (device.device_id)}
        <li class={cn('px-5 py-3.5', device.revoked_at && 'opacity-60')}>
          <div class="flex items-start gap-3">
            <span class={cn('grid size-9 shrink-0 place-items-center rounded-xl', device.current ? 'bg-pale text-forest' : 'bg-surface-2 text-faint')}>
              <Icon name="device" />
            </span>
            <div class="min-w-0 flex-1">
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
                <button class={buttonClass('secondary', 'xs')} disabled={busy} aria-expanded={editing === device.device_id} onclick={() => edit(device)} data-testid="device-capabilities">
                  {t('capabilities')}
                </button>
                {#if !device.current}
                  <button class={buttonClass('danger', 'xs')} disabled={busy} data-testid="revoke-device" onclick={() => ask('device/revoke', { device: device.device_id })}>
                    {t('revoke')}
                  </button>
                {/if}
              </div>
            {/if}
          </div>
          {#if editing === device.device_id}
            <div class="ms-12 mt-3 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-xl border border-line bg-surface-2/60 p-3">
              {#each CAPABILITIES as capability (capability)}
                <Checkbox checked={chosen.includes(capability)} onCheckedChange={(on) => (chosen = on ? [...chosen, capability] : chosen.filter((c) => c !== capability))}>
                  {capability}
                </Checkbox>
              {/each}
              <span class="flex-1"></span>
              <button
                class={buttonClass('primary', 'sm')}
                disabled={busy}
                data-testid="capabilities-submit"
                onclick={() => ask('device/capabilities', { device: device.device_id, capabilities: chosen })}
              >
                {t('submit')}
              </button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </Section>
{/if}
