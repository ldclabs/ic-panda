<script lang="ts">
  import { onMount } from 'svelte'
  import { t, type Label } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import { buttonClass } from '../components/ui'

  let { status }: { status: any } = $props()
  let items = $state<any[]>([])
  let conflicts = $state<any[]>([])
  let query = $state('')
  let trash = $state(false)
  let error = $state('')
  let busy = $state(false)
  let selected = $state<string | null>(null)
  let checked = $state<Record<string, string>>({})
  /** A new item's fields; the secret goes to the daemon only. */
  let draft = $state({ type: 'note', title: '', tags: '', body: '', username: '', secret: '', url: '' })
  /** The open item's fields while editing; an empty `secret` keeps the old one. */
  let edit = $state<any>(null)
  /** Showing or copying a secret waits for a second, explicit click. */
  let confirming = $state<'reveal' | 'copy' | null>(null)
  let revealed = $state<{ id: string; secret: string } | null>(null)
  let hide: ReturnType<typeof setTimeout> | undefined

  /** Which fields each item type shows; every type has a body and tags. */
  const FIELDS: Record<string, string[]> = {
    note: [],
    login: ['username', 'secret', 'url'],
    api: ['username', 'secret', 'url'],
    key: ['secret'],
    file: []
  }
  const TYPES: Record<string, Label> = { note: 'typeNote', login: 'typeLogin', api: 'typeApi', key: 'typeKey', file: 'typeFile' }
  const tags = (text: string) =>
    text
      .split(',')
      .map((tag) => tag.trim())
      .filter(Boolean)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  async function load() {
    error = ''
    try {
      items = await call('vault/list', { trash, query: query.trim() || undefined })
      conflicts = trash ? [] : await call('vault/conflicts')
    } catch (e) {
      items = []
      conflicts = []
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    }
  }

  const add = () =>
    attempt(async () => {
      if (!draft.title.trim()) return
      const now = Date.now()
      const fields = FIELDS[draft.type]
      await call('vault/put', {
        item: {
          type: draft.type,
          title: draft.title.trim(),
          tags: tags(draft.tags),
          body: draft.body,
          username: fields.includes('username') ? draft.username : '',
          secret: fields.includes('secret') ? draft.secret : '',
          url: fields.includes('url') ? draft.url : '',
          favorite: false,
          createdAt: now,
          updatedAt: now
        }
      })
      draft = { type: draft.type, title: '', tags: '', body: '', username: '', secret: '', url: '' }
      await load()
    })

  function open(id: string) {
    selected = selected === id ? null : id
    edit = null
    confirming = null
    revealed = null
  }

  function startEdit(entry: any) {
    edit = {
      title: entry.item.title,
      tags: entry.item.tags.join(', '),
      body: entry.item.body,
      username: entry.item.username,
      url: entry.item.url,
      secret: ''
    }
  }

  /** The edit replaces the version shown; if another device moved on, it becomes a conflict copy. */
  const save = (entry: any) =>
    attempt(async () => {
      const fields = FIELDS[entry.item.type] ?? []
      const changes: Record<string, unknown> = { title: edit.title.trim(), tags: tags(edit.tags), body: edit.body }
      if (fields.includes('username')) changes.username = edit.username
      if (fields.includes('url')) changes.url = edit.url
      if (fields.includes('secret') && edit.secret) changes.secret = edit.secret
      await call('vault/edit', { id: entry.id, base: entry.revision, changes })
      edit = null
      revealed = null
      await load()
    })

  const star = (entry: any) =>
    attempt(async () => {
      await call('vault/edit', { id: entry.id, base: entry.revision, changes: { favorite: !entry.item.favorite } })
      await load()
    })

  const remove = (id: string) =>
    attempt(async () => {
      await call(trash ? 'vault/restore' : 'vault/delete', { id })
      selected = null
      await load()
    })

  /** Shown for a minute, then hidden again. */
  const reveal = (id: string) =>
    attempt(async () => {
      confirming = null
      const entry: any = await call('vault/show', { id, reveal: true })
      revealed = { id, secret: entry.item.secret }
      clearTimeout(hide)
      hide = setTimeout(() => (revealed = null), 60_000)
    })

  /** The main process copies it; this window never holds the copied secret. */
  const copy = (id: string) =>
    attempt(async () => {
      confirming = null
      const { clear_after_ms } = await window.dmsg.copySecret(id)
      toast((seconds) => t('copied', { seconds }), { ms: clear_after_ms, countdown: true, testid: 'secret-copied' })
    })

  /** Files enter and leave through system dialogs; the window never names a path. */
  const fileAction = (work: () => Promise<unknown>) =>
    attempt(async () => {
      await work()
      await load()
    })

  const verify = (id: string) =>
    fileAction(async () => {
      const manifest: any = await call('file/verify', { id })
      checked[id] = `${t('verified')} · ${t('bytes', { n: manifest.size })} · sha256 ${String(manifest.sha256).slice(0, 16)}…`
    })

  /** Keep the current version, or adopt the copy another device wrote. */
  const resolve = (copy: any, keep: 'current' | 'theirs') =>
    attempt(async () => {
      await call('vault/resolve', { key: `${copy.id}:${copy.revision}`, keep })
      await load()
    })

  /** The fields in which a conflict copy differs from the current version. */
  function differences(copy: any): string {
    const head = items.find((e) => e.id === copy.id)
    if (!head) return t('deleted')
    const names: string[] = ['title', 'body', 'username', 'url', 'tags', 'secret', 'favorite'].filter(
      (name) => JSON.stringify(head.item[name]) !== JSON.stringify(copy.item[name])
    )
    if (copy.deleted !== head.deleted) names.push(t('deleted'))
    return names.join(', ') || '-'
  }

  onMount(() => {
    void load()
    const off = window.dmsg.onEvent((event) => {
      if (['vault/changed', 'session/changed', 'sync/changed'].includes(event.type)) void load()
    })
    return () => {
      off()
      clearTimeout(hide)
    }
  })

  const unlocked = $derived(status?.state === 'unlocked')
</script>

<div class="row" style="margin-bottom: 12px">
  <input placeholder={t('search')} bind:value={query} oninput={() => load()} />
  <button class={buttonClass(trash ? 'primary' : 'secondary')} onclick={() => ((trash = !trash), (selected = null), load())}>{t('trash')}</button>
</div>
{#if error}<p class="muted" data-testid="vault-error">{error}</p>{/if}
{#if conflicts.length}
  <div class="card" data-testid="vault-conflicts">
    <h2>{t('conflicts')}</h2>
    <p class="muted">{t('conflictsHint')}</p>
    <ul class="list">
      {#each conflicts as copy (`${copy.id}:${copy.revision}`)}
        <li>
          <div class="row spread">
            <span>
              <strong>{copy.item.title}</strong>
              <span class="muted">{copy.deleted ? ` · ${t('deleted')}` : ''} · {when(copy.item.updatedAt)}</span>
            </span>
            <span class="row">
              <button class={buttonClass()} disabled={busy || !unlocked} onclick={() => resolve(copy, 'current')}>{t('keepCurrent')}</button>
              <button class={buttonClass('primary')} disabled={busy || !unlocked} onclick={() => resolve(copy, 'theirs')}>{t('useCopy')}</button>
            </span>
          </div>
          <div class="muted">{t('differs')}: {differences(copy)}</div>
          {#if copy.item.body}<div class="muted">{copy.item.body}</div>{/if}
        </li>
      {/each}
    </ul>
  </div>
{/if}
<ul class="list card" data-testid="vault-list">
  {#each items as entry (entry.id)}
    <li>
      <div class="row spread">
        <button class="link" onclick={() => open(entry.id)}>
          <strong>{entry.item.favorite ? '★ ' : ''}{entry.item.title}</strong>
          <span class="muted">
            {t(TYPES[entry.item.type] ?? 'typeNote')}{entry.item.tags.length ? ` · ${entry.item.tags.join(', ')}` : ''}{entry.synced
              ? ''
              : ' · ↻'}{conflicts.some((c) => c.id === entry.id) ? ` · ${t('conflicts')}` : ''}
          </span>
        </button>
        <button class={buttonClass('danger')} disabled={busy} onclick={() => remove(entry.id)}>{trash ? t('restore') : t('remove')}</button>
      </div>
      {#if selected === entry.id && !edit}
        <dl style="margin-top: 8px" data-testid="vault-detail">
          {#if entry.item.username}<dt>{t('username')}</dt><dd>{entry.item.username}</dd>{/if}
          {#if entry.item.secret}
            <dt>{t('secret')}</dt>
            <dd class="mono" data-testid="vault-secret">{revealed?.id === entry.id ? revealed?.secret : entry.item.secret}</dd>
          {/if}
          {#if entry.item.url}<dt>URL</dt><dd>{entry.item.url}</dd>{/if}
          {#if entry.item.body}<dt>{t('body')}</dt><dd class="body">{entry.item.body}</dd>{/if}
          {#if entry.item.tags.length}<dt>{t('tags')}</dt><dd>{entry.item.tags.join(', ')}</dd>{/if}
          {#if entry.item.file}<dt>{t('typeFile')}</dt><dd>{entry.item.file.name} ({t('bytes', { n: entry.item.file.size })})</dd>{/if}
          <dt>{t('updated')}</dt>
          <dd>{when(entry.item.updatedAt)} <span class="mono muted">{entry.revision.slice(0, 12)}</span></dd>
        </dl>
        {#if !trash}
          <div class="row" style="margin-top: 8px">
            {#if unlocked}
              <button class={buttonClass()} disabled={busy} onclick={() => startEdit(entry)}>{t('edit')}</button>
              <button class={buttonClass()} disabled={busy} onclick={() => star(entry)}>{entry.item.favorite ? t('unstar') : t('star')}</button>
            {/if}
            {#if entry.item.secret && unlocked}
              {#if revealed?.id === entry.id}
                <button class={buttonClass()} onclick={() => (revealed = null)}>{t('hideSecret')}</button>
              {:else}
                <button class={buttonClass()} data-testid="reveal-secret" onclick={() => (confirming = 'reveal')}>{t('showSecret')}</button>
              {/if}
              <button class={buttonClass()} data-testid="copy-secret" onclick={() => (confirming = 'copy')}>{t('copySecret')}</button>
            {/if}
            {#if entry.item.file}
              <button class={buttonClass()} onclick={() => fileAction(() => window.dmsg.exportFile(entry.id, entry.item.file.name))}>{t('exportFile')}</button>
              <button class={buttonClass()} onclick={() => verify(entry.id)}>{t('verifyFile')}</button>
              {#if checked[entry.id]}<span class="muted" data-testid="file-checked">{checked[entry.id]}</span>{/if}
            {/if}
          </div>
          {#if confirming}
            <div class="row confirm" data-testid="secret-confirm">
              <span class="warn">{confirming === 'reveal' ? t('confirmReveal') : t('confirmCopy')}</span>
              <button class={buttonClass('primary')} data-testid="secret-confirm-yes" disabled={busy} onclick={() => (confirming === 'reveal' ? reveal(entry.id) : copy(entry.id))}>
                {confirming === 'reveal' ? t('showSecret') : t('copySecret')}
              </button>
              <button class={buttonClass()} onclick={() => (confirming = null)}>{t('cancel')}</button>
            </div>
          {/if}
        {/if}
      {/if}
      {#if selected === entry.id && edit}
        <div class="form" data-testid="vault-edit">
          <input placeholder={t('title')} bind:value={edit.title} />
          {#if FIELDS[entry.item.type]?.includes('username')}<input placeholder={t('username')} bind:value={edit.username} />{/if}
          {#if FIELDS[entry.item.type]?.includes('url')}<input placeholder="URL" bind:value={edit.url} />{/if}
          {#if FIELDS[entry.item.type]?.includes('secret')}
            <input type="password" placeholder={t('newSecret')} bind:value={edit.secret} autocomplete="off" />
          {/if}
          <input placeholder={t('tagsHint')} bind:value={edit.tags} />
          <textarea placeholder={t('body')} bind:value={edit.body}></textarea>
          <div class="row">
            <button class={buttonClass('primary')} disabled={busy || !edit.title.trim()} data-testid="vault-save" onclick={() => save(entry)}>{t('save')}</button>
            <button class={buttonClass()} onclick={() => (edit = null)}>{t('cancel')}</button>
          </div>
        </div>
      {/if}
    </li>
  {:else}
    <li class="muted">{t('noItems')}</li>
  {/each}
</ul>
{#if unlocked && !trash}
  <div class="card form">
    <h2>{t('add')}</h2>
    <div class="row">
      <Select bind:value={draft.type} testid="vault-type" options={['note', 'login', 'api', 'key'].map((type) => ({ value: type, label: t(TYPES[type]) }))} />
      <input placeholder={t('title')} bind:value={draft.title} data-testid="vault-title" />
    </div>
    {#if FIELDS[draft.type].includes('username')}<input placeholder={t('username')} bind:value={draft.username} />{/if}
    {#if FIELDS[draft.type].includes('url')}<input placeholder="URL" bind:value={draft.url} />{/if}
    {#if FIELDS[draft.type].includes('secret')}
      <input type="password" placeholder={t('secret')} bind:value={draft.secret} autocomplete="off" data-testid="vault-new-secret" />
    {/if}
    <input placeholder={t('tagsHint')} bind:value={draft.tags} />
    <textarea placeholder={t('body')} bind:value={draft.body}></textarea>
    <div class="row">
      <button class={buttonClass('primary')} disabled={busy} data-testid="vault-add" onclick={add}>{t('save')}</button>
      <button class={buttonClass()} data-testid="import-file" onclick={() => fileAction(() => window.dmsg.importFile())}>{t('importFile')}</button>
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
    text-align: start;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }
  .confirm {
    margin-top: 8px;
  }
  .body {
    white-space: pre-wrap;
  }
</style>
