<script lang="ts">
  import {
    PACKAGE_MIME,
    isSecretPackage,
    parseSecretPackage,
    attachmentName,
    type SecretPackage
  } from '$lib/share/package'
  import {
    ShareDownload,
    authorizeMetadata,
    type MetadataGrant
  } from '$lib/share/download'
  let downloader: ShareDownload | undefined
  let pendingMetadata: Promise<PromiseSettledResult<MetadataGrant>> | undefined
  let counted = false
  let alive = true
  import { page } from '$app/state'
  import SecretSnapshot from '$lib/components/site/SecretSnapshot.svelte'
  import FileInvite from '$lib/components/site/FileInvite.svelte'
  import ShareAuthor from '$lib/components/site/ShareAuthor.svelte'
  import { fileType, readText } from '$lib/share/preview'
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import Glyph from '$lib/components/landing/Glyph.svelte'
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
  import { onMount, onDestroy, tick } from 'svelte'

  // The share page: the link's fragment (or the password) holds the key,
  // the relay holds ciphertext, and the browser does the opening. The
  // download session starts only when the person asks for the file.
  type Phase =
    'loading' | 'password' | 'ready' | 'working' | 'done' | 'gone' | 'error'

  const shareId = $derived(page.params['id'] ?? '')
  let phase = $state<Phase>('loading')
  let descriptor = $state<Descriptor | null>(null)
  const sharerAccount = $derived(
    typeof descriptor?.sharer_account === 'string' &&
      /^[0-9a-v]{20}$/.test(descriptor.sharer_account)
      ? descriptor.sharer_account
      : null
  )
  let key = $state<Uint8Array | null>(null)
  let password = $state('')
  let passwordInput = $state<HTMLInputElement>()
  let file = $state<ShareFile | null>(null)
  let blob = $state<Blob | null>(null)
  let secretPackage = $state<SecretPackage | null>(null)
  const isSnapshot = $derived(file?.mime === PACKAGE_MIME || !!secretPackage)
  let progress = $state({ done: 0, total: 0 })
  let reason = $state('')
  let error = $state('')
  let canRetryMetadata = $state(false)
  let previewText = $state<string | null>(null)
  let formatted = $state<string | null>(null)
  let previewUrl = $state('')
  let previewError = $state('')
  let source = $state(false)
  let headline = $state<HTMLHeadingElement>()
  let reader = $state<HTMLElement>()
  let fullScreen = $state(false)
  let previewDialog = $state<HTMLDialogElement>()
  let fullscreenButton = $state<HTMLButtonElement>()

  async function expandPreview() {
    fullScreen = true
    await tick()
    if (alive) previewDialog?.showModal()
  }

  async function closePreview() {
    fullScreen = false
    await tick()
    if (alive) fullscreenButton?.focus()
  }
  const type = $derived(file ? fileType(file) : null)

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
    if (e && typeof e === 'object' && 'gone' in e) return gone(String(e.gone))
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
    canRetryMetadata = !(e instanceof ShareError)
    phase = file && key && canRetryMetadata ? 'ready' : 'error'
  }

  async function fetchDescriptor(): Promise<Descriptor | null> {
    const response = await fetch(`${API_ORIGIN}/public/shares/${shareId}`)
    if (response.status === 404) return null
    if (!response.ok) throw new Error(`${response.status}`)
    return ((await response.json()) as { data: Descriptor }).data
  }

  async function fetchBytes(path: string): Promise<Uint8Array> {
    if (!alive || !descriptor)
      throw new ShareError('INTEGRITY_FAILED', 'page closed')
    downloader ??= new ShareDownload(API_ORIGIN, descriptor)
    try {
      return await downloader.bytes(path)
    } finally {
      if (alive && descriptor && downloader.hasSession && !counted) {
        counted = true
        descriptor = { ...descriptor, downloads: descriptor.downloads + 1 }
      }
    }
  }

  async function load() {
    phase = 'loading'
    error = ''
    try {
      if (!/^[0-9a-f]{64}$/.test(shareId)) return gone('missing')
      // Settle both outcomes even if a password prompt or invalid link returns early.
      pendingMetadata = authorizeMetadata(API_ORIGIN, shareId).then(
        (value) => ({ status: 'fulfilled', value }),
        (reason: unknown) => ({ status: 'rejected', reason })
      )
      const found = await fetchDescriptor()
      if (!alive) return
      if (!found) return gone('missing')
      descriptor = found
      if (found.status !== 'available') return gone(found.status)
      if (found.password && !key) {
        phase = 'password'
        return
      }
      try {
        key ??= keyFromFragment(location.hash)
      } catch {
        phase = 'error'
        error = $t(
          'The link is missing its key after #. Ask for the whole link.'
        )
        return
      }
      await metadata()
    } catch (e) {
      fail(e)
    }
  }
  onMount(() => {
    void load()
  })

  async function metadata() {
    if (!key || !descriptor) return
    const prefetched = await pendingMetadata
    pendingMetadata = undefined
    if (!alive || !key || !descriptor) return
    if (prefetched?.status === 'rejected') throw prefetched.reason
    downloader ??= new ShareDownload(API_ORIGIN, descriptor)
    const opened = await openManifest(
      key,
      descriptor,
      descriptor.upload_id,
      await downloader.metadata(prefetched?.value)
    )
    if (!alive) return
    file = opened
    phase = 'ready'
  }

  async function unlock() {
    if (!descriptor?.key_wrap || !password) return
    phase = 'working'
    error = ''
    try {
      const unlocked = await unwrapKey(descriptor.key_wrap, password, shareId)
      if (!alive) {
        unlocked.fill(0)
        return
      }
      key = unlocked
      password = ''
      await metadata()
    } catch (e) {
      fail(e)
      if (e instanceof ShareError && e.code === 'WRONG_PASSWORD') {
        await tick()
        if (alive && document.activeElement === document.body) passwordInput?.focus()
      }
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
    const url = URL.createObjectURL(
      new Blob([attachment.bytes as BlobPart], {
        type: 'application/octet-stream'
      })
    )
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = attachmentName(attachment.name)
    anchor.click()
    setTimeout(() => URL.revokeObjectURL(url), 60_000)
  }

  async function download(preview = false) {
    if (!alive || !descriptor || !key || !file || phase === 'working') return
    phase = 'working'
    error = ''
    previewError = ''
    progress = { done: 0, total: descriptor.chunks.length }
    const chunks: Uint8Array[] = []
    let whole: Uint8Array | undefined
    try {
      // Metadata was verified without a counted session. Only content starts one.
      const manifest = file
      if (!alive) return
      for (let i = 0; i < manifest.chunks.length; i++) {
        const bytes = await fetchBytes(`chunks/${i}`)
        if (!alive || !key) return
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
        if (!alive) {
          for (const a of parsed.attachments) a.bytes.fill(0)
          return
        }
        secretPackage = parsed
        whole.fill(0)
        for (const chunk of chunks) chunk.fill(0)
        blob = null
        phase = 'done'
        await tick()
        if (alive) headline?.focus()
        return
      }
      blob = new Blob([whole as BlobPart], {
        type: manifest.mime || 'application/octet-stream'
      })
      if (preview && type?.previewable) {
        if (type.kind === 'image') previewUrl = URL.createObjectURL(blob)
        else {
          try {
            const text = readText(whole)
            if (type.kind === 'markdown') {
              // The verified source is still readable if the formatter can't load.
              const markdown = await import('$lib/share/markdown').catch(() => null)
              if (!alive) return
              formatted = markdown?.renderMarkdown(text) ?? null
            }
            previewText = text
          } catch {
            previewError = $t(
              'This file cannot be previewed here. You can still download it.'
            )
          }
        }
        phase = 'done'
        await tick()
        if (alive) reader?.focus()
      } else {
        phase = 'done'
        save()
      }
    } catch (e) {
      if (!alive) return
      fail(e)
    } finally {
      whole?.fill(0)
      for (const chunk of chunks) chunk.fill(0)
    }
  }
  onDestroy(() => {
    alive = false
    if (previewUrl) URL.revokeObjectURL(previewUrl)
    previewUrl = ''
    previewText = null
    formatted = null
    key?.fill(0)
    key = null
    password = ''
    blob = null
    file = null
    pendingMetadata = undefined
    for (const a of secretPackage?.attachments ?? []) a.bytes.fill(0)
    secretPackage = null
  })
</script>

<svelte:head>
  <title>{$t(isSnapshot ? 'Shared vault item' : 'A file shared with you')}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

{#snippet previewContent(expanded: boolean)}
  <div class="file-actions">
    {#if formatted !== null}
      <button
        class="button secondary"
        aria-pressed={source}
        onclick={() => (source = !source)}
        >{$t(source ? 'Show formatted text' : 'Show source')}</button
      >
    {/if}
    <button class="button secondary" onclick={save} data-testid="share-save"
      ><Glyph name="download" size={18} />{$t('Download')}</button
    >
    {#if expanded}
      <button
        class="button secondary"
        onclick={() => previewDialog?.close()}
        data-testid="share-exit-fullscreen">{$t('Exit full screen')}</button
      >
    {:else if previewText !== null || previewUrl}
      <button
        class="button secondary"
        bind:this={fullscreenButton}
        onclick={expandPreview}
        data-testid="share-fullscreen">{$t('Full screen')}</button
      >
    {/if}
  </div>
  {#if previewError}<p class="utility-status" role="status">{previewError}</p
    >{/if}
  {#if previewText !== null}
    <section
      class="reader"
      aria-label={$t('Read online')}
      tabindex="-1"
      bind:this={reader}
      data-testid="share-reader"
      dir="auto"
    >
      {#if formatted !== null && !source}<div class="markdown"
          >{@html formatted}</div
        >{:else}<pre>{previewText}</pre>{/if}
    </section>
  {:else if previewUrl}
    <div class="image-preview" data-testid="share-image"
      ><img
        src={previewUrl}
        alt={file?.name ?? ''}
        onerror={() => {
          URL.revokeObjectURL(previewUrl)
          previewUrl = ''
          previewError = $t(
            'This file cannot be previewed here. You can still download it.'
          )
        }}
      /></div
    >
  {:else if !previewError}
    <p class="utility-status"
      >{$t('The file checked out and was handed to your browser.')}</p
    >
  {/if}
{/snippet}

<UtilityPage>
  {#snippet aside()}
    <FileInvite kind="share" ready={phase === 'done'} />
  {/snippet}

  <div
    aria-busy={phase === 'working' || phase === 'loading'}
  >
    <div class="utility-icon" aria-hidden="true">{#if isSnapshot}<Glyph name="lock" size={22} />{:else}<IconFile />{/if}</div>
    <h1 bind:this={headline} tabindex="-1" dir="auto">{secretPackage?.item.title ?? $t(isSnapshot ? 'Shared vault item' : 'A file shared with you')}</h1>
    {#if secretPackage}<p class="utility-lead">{$t('Secret snapshot fully verified.')}</p>
    {:else if !isSnapshot}<p class="utility-lead">
      {$t('Someone shared a file with you through dMsg. It is encrypted; your browser decrypts it here with the key from the link, and nothing of it reaches a server in the clear.')}
    </p>{/if}
    <ul class="assurances">
      <li><Glyph name="check" size={14} />{$t('No account needed')}</li>
      <li><Glyph name="check" size={14} />{$t('Nothing to install')}</li>
      <li
        ><Glyph name="lock" size={14} />{$t(
          'Decrypted only in your browser'
        )}</li
      >
    </ul>

    {#if sharerAccount && phase !== 'gone'}
      <ShareAuthor account={sharerAccount} />
    {/if}

    {#if descriptor && phase !== 'gone'}
      <dl class="facts">
        {#if file && !isSnapshot}
          <div><dt>{$t('File')}</dt><dd><bdi>{file.name}</bdi></dd></div>
          <div
            ><dt>{$t('File type')}</dt><dd
              >{type?.kind === 'markdown'
                ? $t('Markdown document')
                : type?.kind === 'text'
                  ? $t('Text document')
                  : type?.kind === 'image'
                    ? $t('Image')
                    : type?.mime}<span class="mime" dir="ltr"
                >{type?.kind ? type.mime : ''}</span
              ></dd
            ></div
          >
          <div><dt>{$t('Size')}</dt><dd>{size(file.size)}</dd></div>
        {:else if !file}
          <div><dt>{$t('Size')}</dt><dd>{size(ciphertextSize)}</dd></div>
        {/if}
        <div>
          <dt>{$t('Expires')}</dt>
          <dd>{new Date(descriptor.expires_at).toLocaleString($locale)}</dd>
        </div>
        {#if descriptor.max_downloads}
          <div>
            <dt>{$t(isSnapshot ? 'Download sessions' : 'Downloads')}</dt>
            <dd>{descriptor.downloads} / {descriptor.max_downloads}</dd>
          </div>
        {/if}
      </dl>
    {/if}

    {#if phase === 'loading'}
      <p class="utility-status" role="status">{$t('Loading…')}</p>
    {:else if phase === 'gone'}
      <p class="utility-status error" role="alert">{reason}</p>
    {:else if phase === 'error'}
      <p class="utility-status error" role="alert">{error}</p>
      {#if canRetryMetadata}<button
          class="button secondary utility-action"
          onclick={load}>{$t('Try again')}</button
        >{/if}
    {:else}
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
            <input
              type="password"
              bind:this={passwordInput}
              bind:value={password}
              autocomplete="off"
              aria-invalid={error ? 'true' : undefined}
              aria-describedby={error ? 'share-password-error' : undefined}
              oninput={() => (error = '')}
            />
          </label>
          {#if error}<p id="share-password-error" class="utility-status error" role="alert">{error}</p
            >{/if}
          <button
            class="button primary utility-action"
            type="submit"
            disabled={!password}>{$t('Open')}</button
          >
        </form>
      {:else if phase === 'ready'}
        {#if isSnapshot}<p class="utility-status" data-testid="snapshot-receive-choice">
            {$t('View the item here or paste this link into dMsg to import it. Each uses one download session.')}
          </p>
        {:else if descriptor?.max_downloads}<p class="utility-status"
            >{$t(
              'Reading or downloading uses one download session. Saving after reading does not use another.'
            )}</p
          >{/if}
        {#if error}<p class="utility-status error" role="alert">{error}</p>{/if}
        {#if !isSnapshot && type?.kind && !type.previewable}<p class="utility-status"
            >{$t(
              'This file is too large to preview here. You can download it instead.'
            )}</p
          >{/if}
        <div class="file-actions">
          {#if isSnapshot}<button class="button primary" onclick={() => download()} data-testid="snapshot-open">{$t('Open')}</button>
          {:else}{#if type?.previewable}<button
              class="button primary"
              onclick={() => download(true)}
              data-testid="share-preview"
              >{$t(type.kind === 'image' ? 'Preview' : 'Read online')}</button
            >{/if}
          <button
            class:primary={!type?.previewable}
            class:secondary={type?.previewable}
            class="button"
            onclick={() => download()}
            data-testid="share-download"
            ><Glyph name="download" size={18} />{$t('Download')}</button
          >{/if}
        </div>
      {:else if phase === 'working'}
        <p class="utility-status" role="status">
          {progress.total
            ? $t('Decrypting… {done} of {total}', {
                done: progress.done,
                total: progress.total
              })
            : $t('Working…')}
        </p>
      {:else if phase === 'done' && secretPackage}
        <SecretSnapshot item={secretPackage.item} />
        {#each secretPackage.attachments as attachment, index}
          <div class="attachment">
            <span><bdi>{attachment.name}</bdi> · {size(attachment.size)}</span>
            <button class="button secondary" onclick={() => saveAttachment(index)}>{$t('Save attachment')}</button>
          </div>
        {/each}
        {#if !secretPackage.item.fields.some((f) => f.value) && !secretPackage.item.notes && !secretPackage.attachments.length}
          <p class="utility-status">{$t('No fields or notes')}</p>
        {/if}
        <p class="utility-status">{$t(descriptor && (!descriptor.max_downloads || descriptor.downloads < descriptor.max_downloads)
          ? 'Paste the link in dMsg to import an independent copy. Revocation cannot delete saved copies.'
          : 'Revoking stops future downloads. Saved copies remain.')}</p>
      {:else if phase === 'done'}
        {#if !fullScreen}{@render previewContent(false)}{/if}
        <dialog
          class="preview-dialog"
          bind:this={previewDialog}
          onclose={closePreview}
          aria-label={$t('Preview')}
        >
          {#if fullScreen}<div class="fullscreen-content">
              <strong class="preview-name"><bdi>{file?.name}</bdi></strong>
              {@render previewContent(true)}
            </div>{/if}
        </dialog>
      {/if}
    {/if}
  </div>
</UtilityPage>

<style>
  .preview-dialog {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100dvh;
    max-width: none;
    max-height: none;
    margin: 0;
    padding: 24px;
    border: 0;
    background: #fff;
    color: var(--ink);
    overflow: auto;
  }
  .preview-dialog::backdrop {
    background: #fff;
  }
  .fullscreen-content {
    max-width: 1000px;
    margin-inline: auto;
  }
  .preview-name {
    display: block;
    overflow-wrap: anywhere;
  }
  .preview-dialog .file-actions {
    position: sticky;
    top: -24px;
    z-index: 1;
    margin-top: 12px;
    padding-block: 12px;
    background: #fff;
  }
  .preview-dialog .reader {
    border: 0;
    padding-inline: 0;
  }
  .preview-dialog .image-preview img {
    max-height: calc(100dvh - 180px);
    margin-inline: auto;
  }
  .assurances {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    margin: -8px 0 0;
    padding: 0;
    color: var(--ink);
    font-size: 13px;
    font-weight: 550;
    list-style: none;
  }
  h1 + .assurances { margin-top: 12px; }
  .assurances li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .assurances :global(svg) {
    flex: none;
    color: var(--action);
  }
  h1, .attachment { overflow-wrap: anywhere; }
  .attachment { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; justify-content: space-between; margin-block: 12px; }
  .file-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 24px;
  }
  .file-actions .button {
    flex: 1 1 160px;
    min-width: 0;
    white-space: normal;
    text-align: center;
  }
  .mime {
    display: block;
    color: var(--muted);
    font-size: 12px;
    font-weight: 400;
  }
  .reader {
    margin-top: 24px;
    padding: 24px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: #fff;
    overflow-wrap: anywhere;
    line-height: 1.8;
  }
  .reader pre {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font:
      14px/1.8 ui-monospace,
      monospace;
  }
  .reader:focus-visible {
    outline: 3px solid var(--action);
    outline-offset: 3px;
  }
  .markdown :global(> :first-child) {
    margin-top: 0;
  }
  .markdown :global(> :last-child) {
    margin-bottom: 0;
  }
  .markdown :global(p),
  .markdown :global(ul),
  .markdown :global(ol),
  .markdown :global(blockquote) {
    margin-block: 1em;
  }
  .markdown :global(h1),
  .markdown :global(h2),
  .markdown :global(h3) {
    margin-block: 1.2em 0.5em;
    line-height: 1.4;
  }
  .markdown :global(h1) {
    font-size: 26px;
  }
  .markdown :global(h2) {
    font-size: 22px;
  }
  .markdown :global(h3) {
    font-size: 18px;
  }
  .markdown :global(ul),
  .markdown :global(ol) {
    padding-inline-start: 24px;
  }
  .markdown :global(pre) {
    white-space: pre-wrap;
    padding: 16px;
    background: #f5f5f5;
    border-radius: 8px;
  }
  .markdown :global(code) {
    font-family: ui-monospace, monospace;
    font-size: 0.9em;
  }
  .markdown :global(blockquote) {
    padding-inline-start: 16px;
    margin-inline: 0;
    border-inline-start: 3px solid var(--line);
  }
  .markdown :global(a) {
    color: var(--action);
    text-decoration: underline;
  }
  .markdown :global(table) {
    display: block;
    overflow-x: auto;
    border-collapse: collapse;
  }
  .markdown :global(th),
  .markdown :global(td) {
    border: 1px solid var(--line);
    padding: 8px 12px;
  }
  .image-preview {
    margin-top: 24px;
    text-align: center;
  }
  .image-preview img {
    display: block;
    max-width: 100%;
    max-height: 75vh;
    height: auto;
    margin-inline: auto;
    object-fit: contain;
  }
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
  @media (max-width: 540px) {
    .reader {
      padding: 16px;
    }
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
