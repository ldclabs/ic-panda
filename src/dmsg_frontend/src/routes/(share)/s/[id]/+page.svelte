<script lang="ts">
  import { PACKAGE_MIME, isSecretPackage, parseSecretPackage, attachmentName, type SecretPackage } from '$lib/share/package'
  import { ShareDownload } from '$lib/share/download'
  let downloader: ShareDownload | undefined
  let alive = true
  import { page } from '$app/state'
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import IconFile from '$lib/components/icons/IconArticleLine.svelte'
  import { API_ORIGIN } from '$lib/constants'
  import { locale, t } from '$lib/i18n'
  import {
    assemble,
    isEmptyPlaceholder,
    keyFromFragment,
    openChunk,
    openManifest,
    ShareError,
    unwrapKey,
    type Descriptor,
    type ShareFile
  } from '$lib/share/decrypt'
  import { onMount, onDestroy } from 'svelte'

  // The share page: the link's fragment (or the password) holds the key,
  // the relay holds ciphertext, and the browser does the opening. The
  // download session starts only when the person asks for the file.
  type Phase =
    'loading' | 'password' | 'ready' | 'working' | 'done' | 'gone' | 'error'

  const shareId = $derived(page.params['id'] ?? '')
  let phase = $state<Phase>('loading')
  let descriptor = $state<Descriptor | null>(null)
  let key = $state<Uint8Array | null>(null)
  let password = $state('')
  let file = $state<ShareFile | null>(null)
  /** The verified manifest reused when a download resumes. */
  let opened: ShareFile | null = null
  let blob = $state<Blob | null>(null)
  let secretPackage = $state<SecretPackage | null>(null)
  let visible = $state<Set<string>>(new Set())
  let showNotes = $state(false)
  let progress = $state({ done: 0, total: 0 })
  let reason = $state('')
  let error = $state('')

  const ciphertextSize = $derived(
    descriptor ? descriptor.chunks.reduce((n, c) => n + c.size, 0) : 0
  )

  function size(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`
    const units = ['KB', 'MB', 'GB']
    let value = bytes / 1024
    let unit = 0
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024
      unit++
    }
    return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`
  }

  function gone(status: string) {
    if (!alive) return
    phase = 'gone'
    reason =
      status === 'expired'
        ? $t('This link has expired.')
        : status === 'revoked'
          ? $t('This link was revoked by the person who shared it.')
          : status === 'exhausted'
            ? $t('This link has reached its download limit.')
            : $t(
                'This link names nothing. Check that you copied the whole of it.'
              )
  }

  function fail(e: unknown) {
    if (!alive) return
    if (e instanceof ShareError && e.code === 'WRONG_PASSWORD') {
      phase = 'password'
      error = $t('Wrong password')
      return
    }
    error =
      e instanceof ShareError
        ? $t('The file did not check out; the download was discarded.')
        : e instanceof TypeError
          ? $t(
              'The relay could not be reached. Check your connection and try again.'
            )
          : $t('Something went wrong: {error}', { error: String(e) })
    // A download that broke off may start again; a file that did not check
    // out may not.
    phase = key && !(e instanceof ShareError) ? 'ready' : 'error'
  }

  async function fetchDescriptor(): Promise<Descriptor | null> {
    const response = await fetch(`${API_ORIGIN}/public/shares/${shareId}`)
    if (response.status === 404) return null
    if (!response.ok) throw new Error(`${response.status}`)
    return ((await response.json()) as { data: Descriptor }).data
  }

  async function fetchBytes(path: string): Promise<Uint8Array> {
    if (!alive || !descriptor) throw new ShareError('INTEGRITY_FAILED','page closed')
    downloader ??= new ShareDownload(API_ORIGIN, descriptor)
    return downloader.bytes(path)
  }

  onMount(async () => {
    try {
      if (!/^[0-9a-f]{64}$/.test(shareId)) return gone('missing')
      const found = await fetchDescriptor()
      if (!alive) return
      if (!found) return gone('missing')
      descriptor = found
      if (found.status !== 'available') return gone(found.status)
      if (found.password) {
        phase = 'password'
        return
      }
      try {
        key = keyFromFragment(location.hash)
      } catch {
        phase = 'error'
        error = $t(
          'The link is missing its key after #. Ask for the whole link.'
        )
        return
      }
      phase = 'ready'
    } catch (e) {
      fail(e)
    }
  })

  async function unlock() {
    if (!descriptor?.key_wrap || !password) return
    phase = 'working'
    error = ''
    try {
      const unlocked = await unwrapKey(descriptor.key_wrap, password, shareId)
      if(!alive){unlocked.fill(0);return}
      key = unlocked
      password = ''
      phase = 'ready'
    } catch (e) {
      fail(e)
    }
  }

  function save() {
    if (!alive || !blob || !file || secretPackage) return
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = file.name
    anchor.click()
    setTimeout(() => URL.revokeObjectURL(url), 60_000)
  }

  function saveAttachment(index: number) {
    const attachment = secretPackage?.attachments[index]
    if (!alive || !attachment) return
    const url = URL.createObjectURL(new Blob([attachment.bytes as BlobPart], { type: 'application/octet-stream' }))
    const anchor = document.createElement('a'); anchor.href = url; anchor.download = attachmentName(attachment.name); anchor.click()
    setTimeout(() => URL.revokeObjectURL(url), 60_000)
  }

  async function download() {
    if (!alive || !descriptor || !key || phase==='working') return
    phase = 'working'
    error = ''
    progress = { done: 0, total: descriptor.chunks.length }
    const chunks: Uint8Array[] = []
    let whole: Uint8Array | undefined
    try {
      // A retry reuses the verified manifest and the same counted download session.
      const manifest = (opened ??= await openManifest(
        key,
        descriptor,
        descriptor.upload_id,
        await fetchBytes('manifest')
      ))
      if (!alive) return
      for (let i = 0; i < manifest.chunks.length; i++) {
        const bytes = await fetchBytes(`chunks/${i}`)
        if(!alive || !key)return
        chunks.push(await openChunk(key, manifest, i, bytes))
        progress = { done: i + 1, total: manifest.chunks.length }
      }
      if (manifest.size === 0) {
        const placeholder = await fetchBytes('chunks/0')
        if (!isEmptyPlaceholder(placeholder))
          throw new ShareError('INTEGRITY_FAILED', 'empty')
      }
      whole = await assemble(manifest, chunks)
      if (!alive) return
      file = manifest
      if (manifest.mime === PACKAGE_MIME || isSecretPackage(whole)) {
        const parsed = await parseSecretPackage(whole)
        if(!alive){for(const a of parsed.attachments)a.bytes.fill(0);return}
        secretPackage = parsed
        whole.fill(0); for (const chunk of chunks) chunk.fill(0)
        blob = null; phase = 'done'; return
      }
      blob = new Blob([whole as BlobPart], {
        type: manifest.mime || 'application/octet-stream'
      })
      phase = 'done'
      save()
    } catch (e) {
      if(!alive)return
      if (e && typeof e === 'object' && 'gone' in e) return gone(String(e.gone))
      fail(e)
    } finally {
      whole?.fill(0);for(const chunk of chunks)chunk.fill(0)
    }
  }
  onDestroy(()=>{alive=false;key?.fill(0);key=null;password='';blob=null;opened=null;file=null;visible=new Set();for(const a of secretPackage?.attachments??[])a.bytes.fill(0);secretPackage=null})
</script>

<svelte:head>
  <title>{$t('A file shared with you')}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<UtilityPage>
  <div
    aria-live="polite"
    aria-busy={phase === 'working' || phase === 'loading'}
  >
    <div class="utility-icon" aria-hidden="true"><IconFile /></div>
    <h1>{$t('A file shared with you')}</h1>
    <p class="utility-lead">
      {$t(
        'Someone shared a file with you through dMsg. It is encrypted; your browser decrypts it here with the key from the link, and nothing of it reaches a server in the clear.'
      )}
    </p>

    {#if phase === 'loading'}
      <p class="utility-status">{$t('Loading…')}</p>
    {:else if phase === 'gone'}
      <p class="utility-status error" role="alert">{reason}</p>
    {:else if phase === 'error'}
      <p class="utility-status error" role="alert">{error}</p>
    {:else}
      {#if descriptor}
        <dl class="facts">
          {#if file}
            <div><dt>{$t('File')}</dt><dd>{file.name}</dd></div>
            <div><dt>{$t('Size')}</dt><dd>{size(file.size)}</dd></div>
          {:else}
            <div><dt>{$t('Size')}</dt><dd>{size(ciphertextSize)}</dd></div>
          {/if}
          <div>
            <dt>{$t('Expires')}</dt>
            <dd>{new Date(descriptor.expires_at).toLocaleString($locale)}</dd>
          </div>
          {#if descriptor.max_downloads}
            <div>
              <dt>{$t('Downloads')}</dt>
              <dd>{descriptor.downloads} / {descriptor.max_downloads}</dd>
            </div>
          {/if}
        </dl>
      {/if}

      {#if phase === 'password'}
        <form
          class="password"
          onsubmit={(event) => {
            event.preventDefault()
            void unlock()
          }}
        >
          <label>
            <span>{$t('This link needs a password.')}</span>
            <input type="password" bind:value={password} autocomplete="off" />
          </label>
          {#if error}<p class="utility-status error" role="alert">{error}</p
            >{/if}
          <button
            class="button primary utility-action"
            type="submit"
            disabled={!password}>{$t('Open')}</button
          >
        </form>
      {:else if phase === 'ready'}
        <p class="utility-status">{$t('Receiving here uses a download session. Importing in dMsg uses a separate session.')}</p>
        {#if error}<p class="utility-status error" role="alert">{error}</p>{/if}
        <button class="button primary utility-action" onclick={download}
          >{$t('Download')}</button
        >
      {:else if phase === 'working'}
        <p class="utility-status">
          {progress.total
            ? $t('Decrypting… {done} of {total}', {
                done: progress.done,
                total: progress.total
              })
            : $t('Working…')}
        </p>
      {:else if phase === 'done' && secretPackage}
        <h2>{secretPackage.item.title}</h2>
        <p>{$t('Secret snapshot fully verified.')}</p>
        {#each secretPackage.item.fields as field (field.id)}
          <div class="facts">
            <strong>{field.section} · {field.label || field.id}</strong>
            <button class="button secondary" onclick={() => { const next = new Set(visible); next.has(field.id) ? next.delete(field.id) : next.add(field.id); visible = next }}>{visible.has(field.id) ? $t('Hide') : $t('Reveal')}</button>
            {#if visible.has(field.id)}<pre style="white-space: pre-wrap; overflow-wrap: anywhere">{field.value}</pre><button class="button secondary" onclick={() => navigator.clipboard.writeText(field.value)}>{$t('Copy')}</button>{/if}
          </div>
        {/each}
        {#if secretPackage.item.notes !== undefined}<button class="button secondary" onclick={() => showNotes = !showNotes}>{$t('Notes')}</button>{#if showNotes}<pre style="white-space: pre-wrap; overflow-wrap: anywhere">{secretPackage.item.notes}</pre>{/if}{/if}
        {#each secretPackage.attachments as attachment, index}
          <p>{attachment.name} · {size(attachment.size)} <button class="button secondary" onclick={() => saveAttachment(index)}>{$t('Save attachment')}</button></p>
        {/each}
        <p>{$t('Paste the link in dMsg to import an independent copy. Revocation cannot delete saved copies.')}</p>
      {:else if phase === 'done'}
        <p class="utility-status"
          >{$t('The file checked out and was handed to your browser.')}</p
        >
        <button class="button secondary utility-action" onclick={save}
          >{$t('Save again')}</button
        >
      {/if}
    {/if}
  </div>
</UtilityPage>

<style>
  .facts {
    display: grid;
    gap: 14px;
    margin: 24px 0 0;
    padding: 20px;
    border-radius: 12px;
    background: var(--canvas);
    font-size: 14px;
  }
  .facts div {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 20px;
    align-items: baseline;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
    text-align: end;
    font-weight: 550;
  }
  .password {
    margin-top: 24px;
  }
  .password label {
    display: grid;
    gap: 10px;
    font-size: 14px;
  }
  .password input {
    width: 100%;
    min-width: 0;
    padding: 12px 14px;
    min-height: 48px;
    border: 1px solid #607566;
    border-radius: 8px;
    background: #fff;
    color: var(--ink);
    font: inherit;
  }
  @media (max-width: 400px) {
    .facts div {
      grid-template-columns: minmax(0, 1fr);
      gap: 4px;
    }
    .facts dd {
      text-align: start;
    }
  }
</style>
