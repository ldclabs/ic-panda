<script lang="ts">
  import { onMount } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Progress from '../../components/Progress.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, size, when } from '../../rpc'
  import FolderPicker from './FolderPicker.svelte'
  import { requestStates } from './requests'

  /** What one link received. Contents come down only when asked for, one file at a time. */
  let {
    link,
    unlocked,
    folders,
    onBack,
    onChanged,
    onShowFile
  }: {
    link: any
    unlocked: boolean
    folders: any[]
    onBack: () => void
    onChanged: () => void
    onShowFile: (id: string) => void
  } = $props()

  let files = $state<any[]>([])
  let loaded = $state(false)
  let busy = $state(false)
  let error = $state('')
  let saving = $state<{ upload: string; stage: string; done: number; total: number } | null>(null)
  let deleting = $state<any | null>(null)
  let choosing = $state<string | null>(null)
  let revision = 0
  let alive = true
  const current = (at: number) => alive && unlocked && at === revision

  const percentOf = (now: { done: number; total: number }) =>
    now.total ? Math.floor((now.done / now.total) * 100) : 0
  const replaced = (id: string) => files.find((file) => file.upload_id === id)?.name

  async function refresh() {
    if (!unlocked || busy || saving) return
    const at = ++revision
    busy = true
    error = ''
    try {
      const next = await call<any[]>('files/request_files', { request_id: link.request_id })
      if (current(at)) {
        files = next
        loaded = true
      }
    } catch (e) {
      if (current(at)) error = errorText(e)
    } finally {
      if (current(at)) busy = false
    }
  }

  async function save(upload: string, folder?: string) {
    if (saving || busy || !unlocked) return
    const at = ++revision
    error = ''
    saving = { upload, stage: 'download', done: 0, total: 0 }
    try {
      await call('files/request_save', {
        upload_id: upload,
        root: t('requestRootFolder'),
        ...(folder ? { folder } : {})
      })
      if (!current(at)) return
      saving = null
      await refresh()
      onChanged()
    } catch (e) {
      if (current(at)) {
        saving = null
        error = errorText(e)
      }
    }
  }

  async function remove() {
    if (!deleting || busy || saving || !unlocked) return
    const at = ++revision
    busy = true
    error = ''
    try {
      await call('files/request_delete_file', { upload_id: deleting.upload_id })
      if (!current(at)) return
      deleting = null
      busy = false
      await refresh()
      onChanged()
    } catch (e) {
      if (current(at)) {
        error = errorText(e)
        busy = false
      }
    }
  }

  onMount(() => {
    void refresh()
    const offWork = window.dmsg.onEvent((event) => {
      const data = (event.data ?? {}) as any
      // Only a new arrival reads the relay; other library changes don't concern this list.
      if (event.type === 'files/changed' && data.method === 'file-request' && !saving && !busy) void refresh()
      if (event.type === 'files/progress' && data.job === 'request' && saving?.upload === data.path)
        saving = { upload: data.path, stage: data.stage, done: data.done, total: data.total }
    })
    return () => {
      alive = false
      revision++
      offWork()
    }
  })
</script>

<section class="flex h-full min-w-0 flex-col" data-testid="request-detail">
  <header class="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
    <button class={buttonClass('ghost', 'sm')} onclick={onBack} data-testid="request-back">
      <Icon name="back" class="rtl:rotate-180" />{t('requestBack')}
    </button>
    <div class="min-w-0 flex-1">
      <h2 class="break-words text-[15px] font-semibold"><bdi dir="auto">{link.title ?? link.request_id}</bdi></h2>
      <p class="mt-0.5 text-[12px] text-faint">
        {#if link.label}<bdi dir="auto">{link.label}</bdi>{' · '}{/if}{t(requestStates[link.status] ?? 'requestStateOpen')} · {t('linkExpires', { time: when(link.expires_at) })}
      </p>
    </div>
    <button class={buttonClass('secondary')} disabled={busy || !!saving || !unlocked} onclick={refresh} data-testid="request-refresh"
      ><Icon name="sync" />{t('refreshLinks')}</button
    >
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto p-5">
    {#if error}<Notice failure={error} tone="danger" testid="request-detail-error">{error}</Notice>{/if}
    {#if link.instructions}
      <p class="mb-3 whitespace-pre-wrap break-words rounded-lg bg-surface-2 px-3 py-2 text-[12.5px] text-muted"><bdi dir="auto">{link.instructions}</bdi></p>
    {/if}
    {#if loaded}
      <ul class="divide-y divide-line" data-testid="request-files">
        {#each files as file (file.upload_id)}
          <li class="grid min-w-0 gap-1.5 py-3" data-upload={file.upload_id} data-state={file.state}>
            <div class="flex min-w-0 flex-wrap items-center gap-3">
              <Icon name={file.state === 'unreadable' ? 'warning' : 'file'} class="shrink-0 text-faint" />
              <div class="min-w-0 basis-48 flex-1">
                {#if file.name}
                  <p class="break-words text-[13px] font-medium"><bdi dir="auto">{file.name}</bdi></p>
                {:else}
                  <Id id={file.upload_id} />
                {/if}
                <p class="mt-0.5 text-[11.5px] text-faint">
                  {size(file.size ?? file.bytes)} · {when(file.stored_at)}
                </p>
                {#if file.sender?.name}
                  <p class="mt-0.5 break-words text-[12px] text-muted" data-testid="request-sender">
                    {t('requestFrom')} <bdi dir="auto">{file.sender.name}</bdi>
                  </p>
                {/if}
                {#if file.sender?.note}
                  <p class="mt-0.5 whitespace-pre-wrap break-words text-[12px] text-muted"><bdi dir="auto">{file.sender.note}</bdi></p>
                {/if}
                {#if file.replaces}
                  <p class="mt-0.5 break-words text-[11.5px] text-faint">
                    {t('requestReplaces', { name: replaced(file.replaces) ?? file.replaces.slice(0, 8) })}
                  </p>
                {/if}
                {#if file.state === 'unreadable'}
                  <p class="mt-0.5 text-[12px] text-warn" data-testid="request-unreadable">{t('requestUnreadable')}</p>
                {:else if file.state === 'withdrawn'}
                  <p class="mt-0.5 text-[12px] text-faint">{t('requestWithdrawn')}</p>
                {:else if file.state === 'saved'}
                  <p class="mt-0.5 text-[12px] text-ok">{t('requestSaved')}</p>
                {/if}
              </div>
              <div class="flex flex-wrap gap-1">
                {#if file.state === 'received'}
                  <button
                    class={buttonClass('secondary', 'xs')}
                    disabled={!!saving || busy || !unlocked}
                    onclick={() => save(file.upload_id)}
                    data-testid="request-save">{t('requestSave')}</button
                  >
                  <button
                    class={buttonClass('ghost', 'xs')}
                    disabled={!!saving || busy || !unlocked}
                    onclick={() => (choosing = file.upload_id)}
                    data-testid="request-save-to">{t('requestSaveTo')}</button
                  >
                {/if}
                {#if file.saved_file}
                  <button class={buttonClass('ghost', 'xs')} onclick={() => onShowFile(file.saved_file)} data-testid="request-show">{t('requestShow')}</button>
                {/if}
                {#if file.state !== 'withdrawn'}
                  <button
                    class={buttonClass('ghost', 'xs', 'text-danger')}
                    disabled={!!saving || busy || !unlocked}
                    onclick={() => {
                      error = ''
                      deleting = file
                    }}
                    data-testid="request-delete">{t('requestDeleteCloud')}</button
                  >
                {/if}
              </div>
            </div>
            {#if saving && saving.upload === file.upload_id}
              {@const step = saving}
              {@const label = t(step.stage === 'save' ? 'requestSaving' : 'requestDownloading', { done: size(step.done), total: size(step.total) })}
              <div class="grid gap-1" data-testid="request-progress">
                <Progress value={percentOf(step)} {label} />
                <p class="text-[11.5px] text-faint" role="status">{label}</p>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if !files.length}<p class="py-12 text-center text-[13px] text-faint" data-testid="request-files-empty">{t('requestFilesEmpty')}</p>{/if}
    {:else if busy}<p class="py-12 text-center text-[13px] text-faint">{t('loading')}</p>{/if}
  </div>
</section>

<FolderPicker
  bind:open={() => !!choosing, (next) => !next && (choosing = null)}
  {folders}
  title={t('requestSaveTo')}
  action={t('requestSave')}
  onPick={(folder) => {
    const upload = choosing
    choosing = null
    if (upload) void save(upload, folder)
  }}
/>

<Dialog bind:open={() => !!deleting, (next) => !next && (deleting = null)} title={t('requestDeleteCloud')} icon="trash" tone="danger" testid="request-delete-confirm">
  <p class="break-words text-[13px] font-medium"><bdi dir="auto">{deleting?.name ?? deleting?.upload_id}</bdi></p>
  <p class="mt-2 text-[13px] text-muted">{t('requestDeleteHint')}</p>
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  {#snippet footer()}<span class="flex-1"></span><button class={buttonClass('ghost')} onclick={() => (deleting = null)}>{t('cancel')}</button><button class={buttonClass('danger')} disabled={busy || !unlocked} onclick={remove} data-testid="request-delete-yes">{t('requestDeleteCloud')}</button>{/snippet}
</Dialog>
