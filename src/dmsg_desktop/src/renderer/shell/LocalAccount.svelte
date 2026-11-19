<script lang="ts">
  import { onDestroy } from 'svelte'
  import Dialog from '../components/Dialog.svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import SecretInput from '../components/SecretInput.svelte'
  import Notice from '../components/page/Notice.svelte'
  import { buttonClass } from '../components/ui'
  import { t, type Label } from '../i18n/index.svelte'
  import { call, code, errorText, size } from '../rpc'
  import { feedback } from '../feedback.svelte'

  type Action = 'create' | 'unlock' | 'password' | 'delete' | 'import'
  type Plan = { id: string; target: string; items: { index: number; title: string; state: string; attachments: (number | null)[] }[]; files: { index: number; name: string; path: string[]; size: number; version: string; local: boolean }[] }
  const labels: Record<Action, Label> = { create: 'useOnDevice', unlock: 'deviceUnlockWorkspace', password: 'localPassword', delete: 'remove', import: 'importLocalAccount' }
  let { action, status, showTrigger = true, refresh = async () => {} }: { action: Action; status: any; showTrigger?: boolean; refresh?: () => Promise<void> } = $props()
  let open = $state(false), busy = $state(false), error = $state(''), password = $state(''), next = $state(''), repeat = $state('')
  let method = $state(''), plan = $state<Plan | null>(null), items = $state<number[]>([]), files = $state<number[]>([])
  let count = $state<{ vault: number; files: number } | null>(null)
  let bytes = $state(0)
  let alive = true
  $effect(() => {
    if (!busy || !plan) return
    const id = plan.id
    const timer = setInterval(async () => {
      try {
        const state = await call<{ bytes?: number }>('local_import/status', { id })
        if (plan?.id === id && state.bytes !== undefined) bytes = state.bytes
      } catch { /* The copy reports its own completion or error. */ }
    }, 750)
    return () => clearInterval(timer)
  })
  const requiredFiles = $derived(new Set(plan?.items.filter(row => items.includes(row.index)).flatMap(row => row.attachments.filter((v): v is number => v !== null))))
  const selectedBytes = $derived(plan?.files.filter(row => files.includes(row.index) || requiredFiles.has(row.index)).reduce((sum, row) => sum + row.size, 0) ?? 0)
  const setting = $derived(method === 'password' && (action === 'password' || action === 'create'))
  const ready = $derived(!busy && (!setting || (!!next && next === repeat)) && (!plan || !!(items.length || files.length)) && (method !== 'password' || action === 'create' || !!password || !!plan))
  function close() {
    if (plan) void call('local_import/cancel', { id: plan.id }).catch(() => {})
    plan = null; password = ''; next = ''; repeat = ''; items = []; files = []; count = null; bytes = 0; open = false
  }
  onDestroy(() => { alive = false; close() })
  export async function begin() {
    error = ''; open = true; busy = true; method = status?.local_unlock_method
    try {
      if (action === 'import') {
        const source = (await call<any[]>('session/accounts')).find(a => a.account_kind === 'local')
        if (!source) throw new Error(t('noResults'))
        method = source.local_unlock_method ?? (source.fast_unlock ? 'hardware' : 'password')
      }
      if (action === 'delete') count = await call('session/local_counts')
    } catch (e) { error = errorText(e) } finally { busy = false }
  }
  async function submit() {
    if (!ready) return
    busy = true; error = ''
    try {
      if (action === 'import' && !plan) {
        const value = await call<Plan>('local_import/preview', method === 'password' ? { password } : {})
        if (!alive) { await call('local_import/cancel', { id: value.id }); return }
        plan = value; password = ''; items = value.items.filter(i => i.state === 'new' && !i.attachments.includes(null)).map(i => i.index)
        return
      }
      if (plan) await call('local_import/commit', { id: plan.id, items, files, folder: 'root' })
      else {
        await feedback.flush()
        await call('session/local_' + action, method === 'password' ? { password: action === 'create' ? next : password, ...(action === 'password' ? { new_password: next } : {}) } : {})
      }
      close(); await refresh()
    } catch (e) {
      if (alive) {
        error = errorText(e)
        if (plan && ['EXPIRED', 'VERSION_CONFLICT'].includes(code(e))) {
          void call('local_import/cancel', { id: plan.id }).catch(() => {})
          plan = null; items = []; files = []; bytes = 0
        }
      }
    } finally { busy = false }
  }
  function toggle(list: number[], index: number, yes: boolean) { return yes ? [...list, index] : list.filter(i => i !== index) }
</script>

{#if showTrigger}<button class={buttonClass(action === 'delete' ? 'danger' : 'secondary', 'sm')} disabled={busy} onclick={begin} data-testid={'local-' + action}>{t(labels[action])}</button>{/if}
<Dialog bind:open={() => open, v => { if (!v) close() }} title={t(labels[action])} description={t(action === 'import' ? 'vaultBackupSync' : 'offlineAccount')} dismissible={!busy || !!plan} testid="local-account-dialog">
  <div class="grid min-w-0 gap-3 [overflow-wrap:anywhere]">
    {#if error}<Notice tone="danger" failure={error}>{error}</Notice>{/if}
    {#if count}<p>{t('vault')} · {count.vault}　{t('files')} · {count.files}</p>{/if}
    {#if plan}
      {#if busy}<output class="text-sm text-muted">{size(bytes)}</output>{/if}
      <p class="text-sm">{t('account')} · <bdi dir="ltr">{plan.target}</bdi></p>
      <h3>{t('vault')}</h3>
      {#each plan.items as row (row.index)}
        {@const omitted = row.attachments.filter(index => index === null).length}
        <Checkbox checked={items.includes(row.index)} disabled={busy || row.state === 'same'} onCheckedChange={v => items = toggle(items, row.index, v)}>
          <bdi>{row.title}</bdi> · {t(row.state === 'same' ? 'vaultBackupSame' : row.state === 'changed' ? 'vaultBackupChanged' : row.state === 'deleted' ? 'vaultBackupDeleted' : 'vaultBackupNew')}
          {#if omitted} · {t('vaultBackupOmitted')} · {omitted}{/if}
        </Checkbox>
      {/each}
      <h3>{t('files')} · {size(selectedBytes)}</h3>
      {#each plan.files as row (row.index)}<Checkbox checked={files.includes(row.index) || requiredFiles.has(row.index)} disabled={busy || !row.local || requiredFiles.has(row.index)} onCheckedChange={v => files = toggle(files, row.index, v)}><bdi>{[...row.path, row.name].join('/')}</bdi> · {size(row.size)} · <bdi dir="ltr">{row.version.slice(0, 8)}</bdi></Checkbox>{/each}
    {:else}
      {#if method === 'password' && action !== 'create'}<SecretInput bind:value={password} disabled={busy} label={t('localPassword')} testid="local-current-password" />{/if}
      {#if setting}
        <SecretInput bind:value={next} disabled={busy} label={t('localPassword')} hint={t('localPasswordLength')} generate testid="local-new-password" />
        <SecretInput bind:value={repeat} disabled={busy} label={t('repeatLocalPassword')} testid="local-confirm-password" />
      {/if}
    {/if}
  </div>
  {#snippet footer()}
    <button class={buttonClass('secondary')} disabled={busy && !plan} onclick={close}>{t('cancel')}</button>
    <button class={buttonClass(action === 'delete' ? 'danger' : 'primary')} disabled={!ready || (action === 'delete' && !count)} aria-busy={busy} onclick={submit} data-testid="local-submit">{t(action === 'delete' ? 'remove' : 'continueOrder')}</button>
  {/snippet}
</Dialog>
