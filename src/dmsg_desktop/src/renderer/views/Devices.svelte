<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { call, code, when } from '../rpc'

  /** The account's devices as the user home certified them; changes are approval requests. */
  let { status }: { status: any } = $props()
  let view = $state<any>(null)
  let editing = $state<string | null>(null)
  let chosen = $state<string[]>([])
  let error = $state('')
  let busy = $state(false)

  const CAPABILITIES = ['ContentSign', 'VaultUnlock', 'RootManage', 'FormalApprove', 'PaymentOffer']

  async function load() {
    try {
      view = await call('account/show')
    } catch (e) {
      view = null
      error = code(e) === 'AUTH_REQUIRED' ? '' : `${t('error')}: ${code(e)}`
    }
  }

  /** The approval window shows the device and decides. */
  async function ask(method: string, params: unknown) {
    busy = true
    error = ''
    try {
      const created: any = await call(method, params)
      editing = null
      await window.dmsg.openApproval(created.id)
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  function edit(device: any) {
    editing = editing === device.device_id ? null : device.device_id
    chosen = [...device.capabilities]
  }

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['approval/changed', 'session/changed', 'sync/changed'].includes(event.type)) void load()
    })
  })

  const short = (id?: string | null) => (id ? id.slice(0, 12) : '-')
</script>

{#if view}
  <div class="card" data-testid="devices">
    <h2>{t('devices')}</h2>
    {#if error}<p class="error">{error}</p>{/if}
    <p class="muted" data-testid="security-snapshot">
      {t('snapshotChecked')}: {t('accountVersion')} {view.account_version} · {t('securityEpoch')} {view.security_epoch} · {t('rootGeneration')}
      {view.root_generation}
    </p>
    <ul class="list">
      {#each view.devices as device (device.device_id)}
        <li>
          <div class="row spread">
            <span>
              <span class="mono">{short(device.device_id)}</span>
              {#if device.current}<span class="badge">{t('thisDevice')}</span>{/if}
              <span class="muted">· {device.role}</span>
              {#if device.revoked_at}<span class="error">· {t('revokedAt')} {when(device.revoked_at)}</span>{/if}
            </span>
            {#if !device.revoked_at && status?.state === 'unlocked'}
              <span class="row">
                <button disabled={busy} onclick={() => edit(device)}>{t('capabilities')}</button>
                {#if !device.current}
                  <button class="danger" disabled={busy} data-testid="revoke-device" onclick={() => ask('device/revoke', { device: device.device_id })}>
                    {t('revoke')}
                  </button>
                {/if}
              </span>
            {/if}
          </div>
          <div class="muted">
            {device.capabilities.join(', ') || '-'} · {t('addedAt')} {when(device.added_at)}{device.added_by
              ? ` · ${t('addedBy')} ${short(device.added_by)}`
              : ''}
          </div>
          {#if editing === device.device_id}
            <div class="row wrap" style="margin-top: 8px">
              {#each CAPABILITIES as capability (capability)}
                <label class="row"><input type="checkbox" value={capability} bind:group={chosen} /> {capability}</label>
              {/each}
              <button
                class="primary"
                disabled={busy}
                data-testid="capabilities-submit"
                onclick={() => ask('device/capabilities', { device: device.device_id, capabilities: chosen })}>{t('submit')}</button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  .wrap {
    flex-wrap: wrap;
  }
</style>
