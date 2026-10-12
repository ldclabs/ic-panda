<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { call, code, when } from '../rpc'

  let { status }: { status: any } = $props()
  let items = $state<any[]>([])
  let query = $state('')
  let trash = $state(false)
  let error = $state('')
  let title = $state('')
  let body = $state('')
  let selected = $state<any>(null)
  let checked = $state<Record<string, string>>({})

  async function load() {
    error = ''
    try {
      items = await call('vault/list', { trash, query: query.trim() || undefined })
    } catch (e) {
      items = []
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    }
  }

  async function add() {
    if (!title.trim()) return
    const now = Date.now()
    await call('vault/put', {
      item: { type: 'note', title, tags: [], body, username: '', secret: '', url: '', favorite: false, createdAt: now, updatedAt: now }
    })
    title = ''
    body = ''
    await load()
  }

  async function remove(id: string) {
    await call(trash ? 'vault/restore' : 'vault/delete', { id })
    selected = null
    await load()
  }

  /** Files enter and leave through system dialogs; the window never names a path. */
  async function fileAction(work: () => Promise<unknown>) {
    error = ''
    try {
      await work()
      await load()
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  }

  const verify = (id: string) =>
    fileAction(async () => {
      const manifest: any = await call('file/verify', { id })
      checked[id] = `${t('verified')} · ${manifest.size} bytes · sha256 ${String(manifest.sha256).slice(0, 16)}…`
    })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (event.type === 'vault/changed' || event.type === 'session/changed') void load()
    })
  })
</script>

<h1>{t('vault')}</h1>
<div class="row" style="margin-bottom: 12px">
  <input placeholder={t('search')} bind:value={query} oninput={() => load()} />
  <button class:primary={trash} onclick={() => ((trash = !trash), load())}>{t('trash')}</button>
</div>
{#if error}<p class="muted" data-testid="vault-error">{error}</p>{/if}
<ul class="list card" data-testid="vault-list">
  {#each items as item (item.id)}
    <li>
      <div class="row spread">
        <button class="link" onclick={() => (selected = item)}>
          <strong>{item.item.title}</strong>
          <span class="muted">{item.item.type}{item.synced ? '' : ' · ↻'}{item.conflict ? ' · conflict' : ''}</span>
        </button>
        <button class="danger" onclick={() => remove(item.id)}>{trash ? t('restore') : t('remove')}</button>
      </div>
      {#if selected?.id === item.id}
        <dl style="margin-top: 8px">
          <dt>{t('body')}</dt>
          <dd>{item.item.body || '-'}</dd>
          {#if item.item.username}<dt>user</dt><dd>{item.item.username}</dd>{/if}
          {#if item.item.url}<dt>url</dt><dd>{item.item.url}</dd>{/if}
          {#if item.item.file}<dt>file</dt><dd>{item.item.file.name} ({item.item.file.size} bytes)</dd>{/if}
          <dt>updated</dt>
          <dd>{when(item.item.updatedAt)}</dd>
        </dl>
        {#if item.item.file && !trash}
          <div class="row">
            <button onclick={() => fileAction(() => window.dmsg.exportFile(item.id, item.item.file.name))}>{t('exportFile')}</button>
            <button onclick={() => verify(item.id)}>{t('verifyFile')}</button>
            {#if checked[item.id]}<span class="muted" data-testid="file-checked">{checked[item.id]}</span>{/if}
          </div>
        {/if}
      {/if}
    </li>
  {:else}
    <li class="muted">{t('noItems')}</li>
  {/each}
</ul>
{#if status?.state === 'unlocked' && !trash}
  <div class="card">
    <h2>{t('add')}</h2>
    <input placeholder={t('title')} bind:value={title} />
    <textarea placeholder={t('body')} bind:value={body} style="margin-top: 8px"></textarea>
    <div class="row" style="margin-top: 8px">
      <button class="primary" onclick={add}>{t('save')}</button>
      <button data-testid="import-file" onclick={() => fileAction(() => window.dmsg.importFile())}>{t('importFile')}</button>
    </div>
  </div>
{/if}

<style>
  .link {
    border: 0;
    background: transparent;
    padding: 0;
    display: flex;
    gap: 10px;
    text-align: left;
  }
</style>
