<script lang="ts">
  import { page } from '$app/state'
  import Brand from '$lib/components/landing/Brand.svelte'
  import '$lib/components/landing/site.css'
  import { API_ORIGIN } from '$lib/constants'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { locale, localeDir, t } from '$lib/i18n'
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
  import { onMount } from 'svelte'

  // The share page: the link's fragment (or the password) holds the key,
  // the relay holds ciphertext, and the browser does the opening. The
  // manifest is read only when the person asks for the file, since the
  // relay counts that as a download.
  type Phase =
    'loading' | 'password' | 'ready' | 'working' | 'done' | 'gone' | 'error'

  const shareId = $derived(page.params['id'] ?? '')
  let phase = $state<Phase>('loading')
  let descriptor = $state<Descriptor | null>(null)
  let key = $state<Uint8Array | null>(null)
  let password = $state('')
  let file = $state<ShareFile | null>(null)
  /** The manifest once opened; the relay counts a download each time it is read. */
  let opened: ShareFile | null = null
  let blob = $state<Blob | null>(null)
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
    const response = await fetch(
      `${API_ORIGIN}/public/shares/${shareId}/${path}`
    )
    if (response.status === 410) {
      const current = await fetchDescriptor()
      throw Object.assign(new Error('gone'), {
        gone: current?.status ?? 'missing'
      })
    }
    if (!response.ok) throw new Error(`${response.status}`)
    return new Uint8Array(await response.arrayBuffer())
  }

  onMount(async () => {
    try {
      if (!/^[0-9a-f]{64}$/.test(shareId)) return gone('missing')
      const found = await fetchDescriptor()
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
      key = await unwrapKey(descriptor.key_wrap, password, shareId)
      password = ''
      phase = 'ready'
    } catch (e) {
      fail(e)
    }
  }

  function save() {
    if (!blob || !file) return
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = file.name
    anchor.click()
    setTimeout(() => URL.revokeObjectURL(url), 60_000)
  }

  async function download() {
    if (!descriptor || !key) return
    phase = 'working'
    error = ''
    progress = { done: 0, total: descriptor.chunks.length }
    try {
      // A retry reuses the manifest already opened: the relay lets the
      // chunks of a download under way be read past its limit.
      const manifest = (opened ??= await openManifest(
        key,
        descriptor,
        descriptor.upload_id,
        await fetchBytes('manifest')
      ))
      const chunks: Uint8Array[] = []
      for (let i = 0; i < manifest.chunks.length; i++) {
        const bytes = await fetchBytes(`chunks/${i}`)
        chunks.push(await openChunk(key, manifest, i, bytes))
        progress = { done: i + 1, total: manifest.chunks.length }
      }
      if (manifest.size === 0) {
        const placeholder = await fetchBytes('chunks/0')
        if (!isEmptyPlaceholder(placeholder))
          throw new ShareError('INTEGRITY_FAILED', 'empty')
      }
      const whole = await assemble(manifest, chunks)
      file = manifest
      blob = new Blob([whole as BlobPart], {
        type: manifest.mime || 'application/octet-stream'
      })
      phase = 'done'
      save()
    } catch (e) {
      if (e && typeof e === 'object' && 'gone' in e) return gone(String(e.gone))
      fail(e)
    }
  }
</script>

<svelte:head>
  <title>{$t('A file shared with you')}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<div class="dmsg-site share-site" lang={$locale} dir={localeDir($locale)}>
  <header class="site-header">
    <div class="wrap nav-wrap">
      <Brand />
      <nav>
        <LocaleSwitcher compact />
      </nav>
    </div>
  </header>
  <main class="wrap share-main">
    <section class="share-card" aria-live="polite">
      <p class="eyebrow">dMsg</p>
      <h1>{$t('A file shared with you')}</h1>
      <p class="lead">
        {$t(
          'Someone shared a file with you through dMsg. It is encrypted; your browser decrypts it here with the key from the link, and nothing of it reaches a server in the clear.'
        )}
      </p>

      {#if phase === 'loading'}
        <p class="status">{$t('Loading…')}</p>
      {:else if phase === 'gone'}
        <p class="status gone">{reason}</p>
      {:else if phase === 'error'}
        <p class="status gone">{error}</p>
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
              <dd>{new Date(descriptor.expires_at).toLocaleString()}</dd>
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
            {#if error}<p class="status gone">{error}</p>{/if}
            <button class="button primary" type="submit" disabled={!password}
              >{$t('Open')}</button
            >
          </form>
        {:else if phase === 'ready'}
          {#if error}<p class="status gone">{error}</p>{/if}
          <button class="button primary" onclick={download}
            >{$t('Download')}</button
          >
        {:else if phase === 'working'}
          <p class="status">
            {progress.total
              ? $t('Decrypting… {done} of {total}', {
                  done: progress.done,
                  total: progress.total
                })
              : $t('Working…')}
          </p>
        {:else if phase === 'done'}
          <p class="status"
            >{$t('The file checked out and was handed to your browser.')}</p
          >
          <button class="button secondary" onclick={save}
            >{$t('Save again')}</button
          >
        {/if}
      {/if}
    </section>
    <p class="about">
      <a class="text-link" href="/" data-sveltekit-reload
        >{$t('What is dMsg?')}</a
      >
    </p>
  </main>
</div>

<style>
  .share-main {
    padding: 56px 0 96px;
  }
  .share-card {
    max-width: 640px;
    margin: 0 auto;
    padding: 40px 36px;
    border: 1px solid var(--line);
    border-radius: 20px;
    background: white;
  }
  .eyebrow {
    margin: 0 0 10px;
    font-size: 13px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--action);
  }
  h1 {
    margin: 0;
    font-size: clamp(28px, 4vw, 40px);
    font-weight: 650;
    letter-spacing: -0.03em;
    line-height: 1.15;
  }
  .lead {
    margin: 16px 0 24px;
    color: var(--muted);
  }
  .facts {
    display: grid;
    gap: 10px;
    margin: 0 0 24px;
    padding: 16px 18px;
    border-radius: 12px;
    background: var(--pale);
  }
  .facts div {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
    text-align: end;
    font-weight: 550;
  }
  .status {
    margin: 0 0 16px;
    color: var(--muted);
  }
  .status.gone {
    color: #b91c1c;
  }
  .password {
    display: grid;
    gap: 14px;
  }
  .password label {
    display: grid;
    gap: 8px;
  }
  .password input {
    padding: 12px 14px;
    border: 1px solid #607566;
    border-radius: 8px;
    font: inherit;
  }
  .about {
    margin: 28px 0 0;
    text-align: center;
  }
</style>
