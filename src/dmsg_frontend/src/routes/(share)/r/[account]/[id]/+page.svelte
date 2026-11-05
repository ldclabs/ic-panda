<script lang="ts">
  import { page } from '$app/state'
  import Glyph from '$lib/components/landing/Glyph.svelte'
  import IconFile from '$lib/components/icons/IconArticleLine.svelte'
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import { installFor } from '$lib/downloads'
  import { locale, t } from '$lib/i18n'
  import { readProfile, type ProfilePage } from '$lib/public-profile'
  import {
    RequestApi,
    RequestApiError,
    type LinkStatus
  } from '$lib/request/api'
  import { readRecords, type SentRecord } from '$lib/request/records'
  import {
    openDescriptor,
    receiptKey,
    uploadToken,
    type Receipt,
    type RequestDescriptor
  } from '$lib/request/seal'
  import {
    uploadFile,
    UploadStop,
    type UploadLink,
    type UploadProgress
  } from '$lib/request/upload'
  import { keyFromFragment, unbase64url } from '$lib/share/decrypt'
  import { onDestroy, onMount } from 'svelte'

  // The page behind a file request link. The fragment's key opens what is being
  // asked and gives the power to encrypt, not to read: files go up sealed to a
  // key only the requester holds, and nothing comes back but a record of them.
  type Phase = 'loading' | 'blocked' | 'ready'
  type Item = {
    id: number
    file: File
    replaces: string | null
    state: 'waiting' | 'working' | 'sent' | 'failed'
    progress: UploadProgress | null
    error: string
    receipt: Receipt | null
    uploadId: string | null
  }

  const HARD_LIMIT = 1000 * 1024 * 1024
  const PHONE_HINT = 100 * 1024 * 1024
  const account = $derived(page.params['account'] ?? '')
  const requestId = $derived(page.params['id'] ?? '')
  const install = installFor(navigator.userAgent, navigator.maxTouchPoints)
  const phone =
    /iPhone|iPad|iPod|Android/.test(navigator.userAgent) ||
    navigator.maxTouchPoints > 1

  let phase = $state<Phase>('loading')
  let blocked = $state('')
  let canRetryLoad = $state(false)
  let status = $state<LinkStatus>('open')
  let descriptor = $state<RequestDescriptor | null>(null)
  let expiresAt = $state(0)
  let maxFileBytes = $state<number | null>(null)
  let requester = $state<ProfilePage | null>(null)
  let name = $state('')
  let note = $state('')
  let items = $state<Item[]>([])
  let records = $state<SentRecord[]>([])
  let persistent = $state(true)
  let sending = $state(false)
  let dragging = $state(false)
  let confirming = $state<string | null>(null)
  let replacing = $state<string | null>(null)
  let notice = $state('')
  let everSent = $state(false)
  let picker = $state<HTMLInputElement>()
  let replacer = $state<HTMLInputElement>()

  let api: RequestApi | null = null
  let link: UploadLink | null = null
  let pageKey: Uint8Array | null = null
  let run: AbortController | null = null
  let lock: { release(): Promise<void> } | null = null
  let nextItem = 0
  const abort = new AbortController()

  const closed = $derived(status === 'expired' || status === 'revoked')
  const waiting = $derived(items.filter((i) => i.state === 'waiting').length)
  const statusText = $derived(
    status === 'full'
      ? $t(
          'This link has no room for more files. Ask the person who sent it for another.'
        )
      : status === 'paused'
        ? $t(
            'This link is not taking files right now. Ask the person who sent it.'
          )
        : status === 'expired'
          ? $t('This link has expired.')
          : status === 'revoked'
            ? $t('This link was closed by the person who sent it.')
            : ''
  )
  const requesterName = $derived(
    requester?.document?.name ||
      (requester?.handle ? `@${requester.handle}` : '')
  )
  const limit = $derived(maxFileBytes ?? HARD_LIMIT)

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

  const when = (at: number) => new Date(at).toLocaleString($locale)

  function blockWith(message: string, retry = false) {
    phase = 'blocked'
    blocked = message
    canRetryLoad = retry
  }

  async function refreshRecords() {
    if (!api || !link) return
    const mine = await api.mine()
    const opened = await readRecords(
      mine.entries,
      link.receiptKey,
      link.requestId
    )
    if (abort.signal.aborted) return
    persistent = mine.persistent
    records = opened
  }

  async function refreshStatus() {
    if (!api) return
    const view = await api.status()
    if (abort.signal.aborted) return
    status = view.status
    if (view.max_file_bytes !== undefined) maxFileBytes = view.max_file_bytes
  }

  async function load() {
    phase = 'loading'
    blocked = ''
    canRetryLoad = false
    try {
      if (!/^[0-9a-f]{64}$/.test(requestId))
        return blockWith(
          $t('This link names nothing. Check that you copied the whole of it.')
        )
      if (!globalThis.crypto?.subtle)
        return blockWith(
          $t(
            'This browser cannot encrypt files here. Open the link in a current browser.'
          )
        )
      try {
        pageKey = keyFromFragment(location.hash)
      } catch {
        return blockWith($t('This link is incomplete. Ask for the whole link.'))
      }
      api = RequestApi.forLink(
        account,
        requestId,
        uploadToken(pageKey, account, requestId)
      )
      const view = await api.status()
      if (abort.signal.aborted) return
      if (!view.descriptor)
        return blockWith($t('This link is incomplete. Ask for the whole link.'))
      const opened = await openDescriptor(
        pageKey,
        account,
        requestId,
        view.descriptor
      ).catch(() => null)
      if (abort.signal.aborted) return
      if (!opened)
        return blockWith($t('This link is incomplete. Ask for the whole link.'))
      descriptor = opened
      status = view.status
      expiresAt = view.expires_at ?? 0
      maxFileBytes = view.max_file_bytes ?? null
      link = {
        account,
        requestId,
        hpkePub: unbase64url(opened.hpke_pub),
        receiptKey: receiptKey(pageKey, account, requestId)
      }
      phase = 'ready'
      void readProfile(account, abort.signal)
        .then((found) => {
          if (!abort.signal.aborted) requester = found
        })
        .catch(() => {})
      await refreshRecords().catch(() => {})
    } catch (e) {
      if (abort.signal.aborted) return
      if (e instanceof RequestApiError && e.status < 500 && e.status !== 429)
        blockWith(
          $t('This link names nothing. Check that you copied the whole of it.')
        )
      else blockWith($t('The connection was lost. Try again.'), true)
    }
  }

  onMount(load)

  function warnLeaving(event: BeforeUnloadEvent) {
    if (sending) event.preventDefault()
  }

  function addFiles(files: Iterable<File>, replaces: string | null = null) {
    notice = ''
    for (const file of files) {
      const refused =
        file.size === 0
          ? $t('This file is empty.')
          : file.size > limit
            ? $t('This file is larger than this link takes ({size}).', {
                size: size(limit)
              })
            : ''
      items.push({
        id: nextItem++,
        file,
        replaces,
        state: refused ? 'failed' : 'waiting',
        progress: null,
        error: refused,
        receipt: null,
        uploadId: null
      })
    }
  }

  function why(e: unknown): string {
    if (e instanceof UploadStop) {
      if (e.reason === 'empty') return $t('This file is empty.')
      if (e.reason === 'changed')
        return $t('The file changed while it was being sent. Choose it again.')
      return ''
    }
    if (e instanceof RequestApiError) {
      if (e.code === 'TOO_LARGE')
        return $t('This file is larger than this link takes ({size}).', {
          size: size(limit)
        })
      if (e.status < 500 && e.status !== 429)
        return statusText || $t('This file could not be sent.')
    }
    if (e instanceof TypeError || e instanceof RequestApiError)
      return $t('The connection was lost. Try again.')
    return $t('This file could not be sent.')
  }

  function senderOf() {
    const [who, words] = [name.trim(), note.trim()]
    return who || words ? { name: who, note: words } : null
  }

  async function sendItem(item: Item, signal: AbortSignal) {
    if (!api || !link) return
    item.state = 'working'
    item.error = ''
    let reserved: string | null = null
    try {
      const sent = await uploadFile(
        api,
        link,
        { file: item.file, sender: senderOf(), replaces: item.replaces },
        {
          signal,
          reserved: (id) => (reserved = id),
          progress: (progress) => (item.progress = progress)
        }
      )
      item.state = 'sent'
      item.receipt = sent.receipt
      item.uploadId = sent.uploadId
      everSent = true
      // The new file is safe; only now does the old one go.
      if (item.replaces)
        await api.withdraw(item.replaces).catch((e) => {
          if (e instanceof RequestApiError && e.message === 'saved')
            notice = $t(
              'They have already saved this file. Contact them directly.'
            )
        })
    } catch (e) {
      const stopped = e instanceof UploadStop && e.reason === 'stopped'
      item.state = stopped ? 'waiting' : 'failed'
      item.progress = null
      item.error = stopped ? '' : why(e)
      if (e instanceof RequestApiError) await refreshStatus().catch(() => {})
      // What the page started and won't finish should not hold the link's room.
      if (reserved) await api.withdraw(reserved).catch(() => {})
    }
  }

  async function sendAll() {
    if (sending || !api) return
    sending = true
    notice = ''
    run = new AbortController()
    lock =
      (await (
        navigator as { wakeLock?: { request(kind: 'screen'): Promise<any> } }
      ).wakeLock
        ?.request('screen')
        .catch(() => null)) ?? null
    try {
      for (const item of items) {
        if (run.signal.aborted) break
        if (item.state === 'waiting') await sendItem(item, run.signal)
      }
      await refreshRecords().catch(() => {})
    } finally {
      sending = false
      run = null
      await lock?.release().catch(() => {})
      lock = null
    }
  }

  function retry(item: Item) {
    item.state = 'waiting'
    item.error = ''
    void sendAll()
  }

  function removeItem(item: Item) {
    items = items.filter((i) => i.id !== item.id)
  }

  async function withdraw(uploadId: string) {
    if (!api) return
    confirming = null
    notice = ''
    try {
      await api.withdraw(uploadId)
    } catch (e) {
      notice =
        e instanceof RequestApiError && e.message === 'saved'
          ? $t('They have already saved this file. Contact them directly.')
          : why(e)
    }
    await refreshRecords().catch(() => {})
  }

  async function clearRecords() {
    if (!api) return
    await api.forget().catch(() => {})
    records = []
    await refreshRecords().catch(() => {})
  }

  function receiptText(uploadId: string, receipt: Receipt) {
    return [
      receipt.name,
      `${receipt.size} B`,
      `SHA-256 ${receipt.sha256}`,
      new Date(receipt.sent_at).toISOString(),
      uploadId
    ].join('\n')
  }

  const stateText = (state: SentRecord['state']) =>
    ({
      uploading: $t('Not finished'),
      delivered: $t('Delivered'),
      saved: $t('Saved by them'),
      withdrawn: $t('Withdrawn'),
      replaced: $t('Replaced'),
      removed: $t('Removed by them')
    })[state]

  onDestroy(() => {
    abort.abort()
    run?.abort()
    pageKey?.fill(0)
    link?.receiptKey.fill(0)
    void lock?.release().catch(() => {})
  })
</script>

<svelte:head>
  <title>{$t('Send files through dMsg')}</title>
  <meta name="robots" content="noindex" />
</svelte:head>
<svelte:window onbeforeunload={warnLeaving} />

<UtilityPage>
  {#snippet aside()}
    {#if phase !== 'loading'}
      <aside
        class="invite"
        class:ready={everSent}
        aria-labelledby="invite-title"
        data-testid="request-invite"
      >
        <h2 id="invite-title">{$t('Collect files this way too')}</h2>
        <p
          >{$t(
            'dMsg is a private workspace for your files, messages and passwords. Ask anyone for files by link, and only you can open them.'
          )}</p
        >
        <ul>
          {#each ['The person you send it to needs no account or app.', 'Each person sees only what they sent.', 'Free to start, with encryption included in every plan.'] as const as point}
            <li><Glyph name="check" size={16} /><span>{$t(point)}</span></li>
          {/each}
        </ul>
        <div class="invite-actions">
          <a
            class="button primary"
            href={install.href}
            data-testid="request-install"
            data-sveltekit-reload
            ><Glyph name="download" size={18} />{$t(install.label)}</a
          >
          <a class="button secondary" href="/" data-sveltekit-reload
            >{$t('See how dMsg works')}</a
          >
        </div>
      </aside>
    {/if}
  {/snippet}

  <div aria-busy={phase === 'loading' || sending}>
    <div class="utility-icon" aria-hidden="true"><IconFile /></div>
    <h1>
      {requesterName
        ? $t('{name} is asking you for files', { name: requesterName })
        : $t('Someone is asking you for files')}
    </h1>
    <p class="utility-lead">
      {$t(
        'Your browser encrypts each file here, so only the person who asked can open it. This link can only upload: nobody can download anything through it.'
      )}
    </p>
    <ul class="assurances">
      <li><Glyph name="check" size={14} />{$t('No account needed')}</li>
      <li><Glyph name="check" size={14} />{$t('Nothing to install')}</li>
      <li
        ><Glyph name="lock" size={14} />{$t(
          'Only they can open your files'
        )}</li
      >
    </ul>

    {#if phase === 'loading'}
      <p class="utility-status" role="status">{$t('Loading…')}</p>
    {:else if phase === 'blocked'}
      <p class="utility-status error" role="alert">{blocked}</p>
      {#if canRetryLoad}
        <button class="button secondary utility-action" onclick={load}
          >{$t('Try again')}</button
        >
      {/if}
    {:else if descriptor}
      <div class="asking" data-testid="request-asking">
        <h2 dir="auto">{descriptor.title}</h2>
        {#if descriptor.instructions}
          <p dir="auto">{descriptor.instructions}</p>
        {/if}
      </div>

      <dl class="facts">
        <div>
          <dt>{$t('Asked by')}</dt>
          <dd>
            {#if requesterName}<span dir="auto">{requesterName}</span>{/if}
            {#if requester?.handle && requester.document?.name}
              <span class="quiet" dir="ltr">@{requester.handle}</span>
            {/if}
            <span class="quiet account" dir="ltr">{account}</span>
            <a
              class="text-link"
              href={`/${account}`}
              target="_blank"
              rel="noopener noreferrer"
              data-testid="request-profile"
              >{$t('Check their public profile')}</a
            >
          </dd>
        </div>
        {#if expiresAt && !closed}
          <div>
            <dt>{$t('Open until')}</dt>
            <dd>{when(expiresAt)}</dd>
          </div>
        {/if}
        {#if maxFileBytes && status === 'open'}
          <div>
            <dt>{$t('Largest file')}</dt>
            <dd dir="ltr">{size(maxFileBytes)}</dd>
          </div>
        {/if}
      </dl>

      {#if statusText}
        <p
          class="utility-status warning"
          role="alert"
          data-testid="request-status">{statusText}</p
        >
      {/if}

      {#if status === 'open'}
        <div class="sender">
          <label>
            <span>{$t('Your name (optional)')}</span>
            <input
              bind:value={name}
              maxlength="80"
              autocomplete="name"
              dir="auto"
              disabled={sending}
              data-testid="request-name"
            />
          </label>
          <label>
            <span>{$t('A note (optional)')}</span>
            <textarea
              bind:value={note}
              maxlength="240"
              rows="2"
              dir="auto"
              disabled={sending}
              data-testid="request-note"></textarea>
          </label>
          <p class="hint"
            >{$t('They see this with your files. dMsg does not verify it.')}</p
          >
        </div>

        <label
          class="drop"
          class:dragging
          ondragover={(event) => {
            event.preventDefault()
            dragging = true
          }}
          ondragleave={() => (dragging = false)}
          ondrop={(event) => {
            event.preventDefault()
            dragging = false
            if (event.dataTransfer) addFiles(event.dataTransfer.files)
          }}
        >
          <Glyph name="download" size={20} />
          <span>{$t('Choose files or drop them here')}</span>
          <input
            bind:this={picker}
            class="picker"
            type="file"
            multiple
            disabled={sending}
            data-testid="request-files"
            onchange={() => {
              if (picker?.files) addFiles(picker.files)
              if (picker) picker.value = ''
            }}
          />
        </label>
      {/if}
      <input
        bind:this={replacer}
        class="picker hidden"
        type="file"
        tabindex="-1"
        aria-hidden="true"
        data-testid="request-replace-file"
        onchange={() => {
          if (replacer?.files) addFiles(replacer.files, replacing)
          if (replacer) replacer.value = ''
          replacing = null
          void sendAll()
        }}
      />

      {#if items.length}
        <ul class="queue" data-testid="request-queue">
          {#each items as item (item.id)}
            <li>
              <div class="row">
                <span class="file" dir="auto">{item.file.name}</span>
                <span class="quiet" dir="ltr">{size(item.file.size)}</span>
              </div>
              {#if item.state === 'working' && item.progress}
                {@const percent = item.progress.total
                  ? Math.floor((item.progress.done / item.progress.total) * 100)
                  : 0}
                <div
                  class="bar"
                  role="progressbar"
                  aria-valuemin="0"
                  aria-valuemax="100"
                  aria-valuenow={percent}
                  aria-label={item.file.name}
                >
                  <i style:inline-size={`${percent}%`}></i>
                </div>
                <p class="line" role="status">
                  {item.progress.stage === 'preparing'
                    ? $t('Preparing… {percent}%', { percent })
                    : $t('Uploading… {percent}%', { percent })}
                </p>
              {:else if item.state === 'waiting'}
                <p class="line">
                  {$t('Waiting')}
                  {#if phone && item.file.size > PHONE_HINT}
                    · {$t(
                      'This is a large file for a phone. A computer may be easier.'
                    )}
                  {/if}
                </p>
              {:else if item.state === 'sent'}
                <p class="line sent" role="status">{$t('Sent')}</p>
              {:else if item.state === 'failed'}
                <p class="line failed" role="alert">
                  {item.error || $t('This file could not be sent.')}
                </p>
              {/if}
              <div class="row-actions">
                {#if item.state === 'failed' && item.file.size > 0 && item.file.size <= limit && status === 'open'}
                  <button
                    class="link"
                    onclick={() => retry(item)}
                    disabled={sending}>{$t('Try again')}</button
                  >
                {/if}
                {#if item.state === 'waiting' || item.state === 'failed'}
                  <button
                    class="link"
                    onclick={() => removeItem(item)}
                    disabled={sending}>{$t('Remove')}</button
                  >
                {/if}
                {#if item.state === 'working'}
                  <button class="link" onclick={() => run?.abort()}
                    >{$t('Stop')}</button
                  >
                {/if}
                {#if item.state === 'sent' && item.receipt && item.uploadId}
                  {@const text = receiptText(item.uploadId, item.receipt)}
                  <button
                    class="link"
                    onclick={() => navigator.clipboard.writeText(text)}
                    >{$t('Copy receipt')}</button
                  >
                {/if}
              </div>
            </li>
          {/each}
        </ul>
        {#if sending}
          <p class="utility-status warning" role="status"
            >{$t('Keep this page open until your files are sent.')}</p
          >
        {:else if waiting && status === 'open'}
          <button
            class="button primary utility-action"
            onclick={sendAll}
            data-testid="request-send">{$t('Send files')}</button
          >
        {/if}
      {/if}

      {#if notice}
        <p class="utility-status error" role="alert">{notice}</p>
      {/if}

      {#if everSent && !persistent}
        <p class="utility-status warning" role="status"
          >{$t(
            'This browser will not keep a record of your uploads. Copy each receipt if you need one.'
          )}</p
        >
      {/if}

      {#if records.length}
        <section class="records" data-testid="request-records">
          <h2>{$t('Sent from this browser')}</h2>
          <p class="hint"
            >{$t(
              'Only this browser remembers these; another browser shows none of them.'
            )}</p
          >
          <ul>
            {#each records as record (record.uploadId)}
              <li data-state={record.state}>
                <div class="row">
                  <span class="file" dir="auto"
                    >{record.receipt?.name ?? '—'}</span
                  >
                  <span class="quiet" dir="ltr"
                    >{size(record.receipt?.size ?? record.bytes)}</span
                  >
                </div>
                <p class="line">
                  {stateText(record.state)} · {when(record.storedAt)}
                </p>
                {#if confirming === record.uploadId}
                  <div class="row-actions">
                    <span class="line">{$t('Withdraw this file?')}</span>
                    <button
                      class="link danger"
                      onclick={() => withdraw(record.uploadId)}
                      >{$t('Withdraw')}</button
                    >
                    <button class="link" onclick={() => (confirming = null)}
                      >{$t('Cancel')}</button
                    >
                  </div>
                {:else if !closed}
                  <div class="row-actions">
                    {#if record.state === 'delivered'}
                      <button
                        class="link"
                        disabled={sending || status !== 'open'}
                        onclick={() => {
                          replacing = record.uploadId
                          replacer?.click()
                        }}>{$t('Replace')}</button
                      >
                      <button
                        class="link"
                        disabled={sending}
                        onclick={() => (confirming = record.uploadId)}
                        >{$t('Withdraw')}</button
                      >
                    {/if}
                    {#if record.state === 'saved' && status === 'open'}
                      <button
                        class="link"
                        disabled={sending}
                        onclick={() => {
                          replacing = null
                          replacer?.click()
                        }}>{$t('Send a new version')}</button
                      >
                    {/if}
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
          <button class="link" onclick={clearRecords} disabled={sending}
            >{$t('Clear this list')}</button
          >
        </section>
      {/if}
    {/if}
  </div>
</UtilityPage>

<style>
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
  .assurances li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .assurances :global(svg) {
    flex: none;
    color: var(--action);
  }
  /* What the requester wrote: framed apart from the page's own words. */
  .asking {
    margin-top: 24px;
    padding: 16px 20px;
    border-inline-start: 3px solid var(--action);
    background: var(--canvas);
    border-radius: 4px;
  }
  .asking h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .asking p {
    margin: 8px 0 0;
    color: var(--ink);
    font-size: 14px;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .facts {
    display: grid;
    gap: 14px;
    margin: 16px 0 0;
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
    display: grid;
    gap: 2px;
    justify-items: end;
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
    text-align: end;
    font-weight: 550;
  }
  .quiet {
    color: var(--muted);
    font-size: 13px;
    font-weight: 400;
  }
  .account {
    font-size: 12px;
    word-break: break-all;
  }
  .sender {
    display: grid;
    gap: 12px;
    margin-top: 24px;
  }
  .sender label {
    display: grid;
    gap: 6px;
    font-size: 14px;
    font-weight: 550;
  }
  .sender input,
  .sender textarea {
    width: 100%;
    min-width: 0;
    min-height: 44px;
    padding: 10px 12px;
    border: 1px solid #607566;
    border-radius: 8px;
    background: #fff;
    color: var(--ink);
    font: inherit;
    font-weight: 400;
    resize: vertical;
  }
  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
  .drop {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
    justify-content: center;
    margin-top: 16px;
    padding: 24px 16px;
    border: 1px dashed #607566;
    border-radius: 12px;
    background: var(--canvas);
    color: var(--ink);
    font-size: 14px;
    font-weight: 550;
    text-align: center;
    cursor: pointer;
  }
  .drop.dragging,
  .drop:hover,
  .drop:focus-within {
    border-color: var(--action);
    background: var(--pale);
  }
  .drop :global(svg) {
    color: var(--action);
    transform: rotate(180deg);
  }
  .picker {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }
  .picker.hidden {
    position: fixed;
    inset: auto;
    width: 1px;
    height: 1px;
    pointer-events: none;
  }
  .queue,
  .records ul {
    display: grid;
    gap: 10px;
    margin: 16px 0 0;
    padding: 0;
    list-style: none;
  }
  .queue li,
  .records li {
    display: grid;
    gap: 6px;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: 10px;
    font-size: 14px;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: baseline;
    justify-content: space-between;
    min-width: 0;
  }
  .file {
    min-width: 0;
    font-weight: 550;
    overflow-wrap: anywhere;
  }
  .row .quiet {
    flex: none;
  }
  .line {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .line.sent {
    color: var(--action);
    font-weight: 550;
  }
  .line.failed {
    color: #991b1b;
  }
  .bar {
    height: 6px;
    overflow: hidden;
    border-radius: 3px;
    background: var(--line);
  }
  .bar i {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--action);
  }
  .row-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    align-items: center;
  }
  .row-actions:empty {
    display: none;
  }
  .link {
    padding: 2px 0;
    min-height: 28px;
    border: 0;
    background: none;
    color: var(--action);
    font: inherit;
    font-size: 13px;
    font-weight: 550;
    text-decoration: underline;
    cursor: pointer;
  }
  .link:disabled {
    color: var(--muted);
    cursor: not-allowed;
  }
  .link.danger {
    color: #991b1b;
  }
  .records {
    display: grid;
    gap: 6px;
    margin-top: 28px;
    padding-top: 20px;
    border-top: 1px solid var(--line);
  }
  .records h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 650;
  }
  .records > .link {
    justify-self: start;
    margin-top: 8px;
  }
  .invite {
    margin-top: 16px;
    padding: 24px 32px 28px;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--pale);
    font-size: 14px;
  }
  .invite.ready {
    border-color: var(--action);
  }
  .invite h2 {
    margin: 0 0 8px;
    font-size: 20px;
    font-weight: 650;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .invite p {
    margin: 0;
    color: var(--ink);
  }
  .invite ul {
    display: grid;
    gap: 8px;
    margin: 16px 0 20px;
    padding: 0;
    list-style: none;
  }
  .invite li {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr);
    gap: 8px;
    align-items: start;
    color: var(--ink);
  }
  .invite li :global(svg) {
    margin-top: 3px;
    color: var(--action);
  }
  .invite-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .invite-actions .button {
    flex: 1 1 200px;
    min-width: 0;
    white-space: normal;
    text-align: center;
  }
  @media (max-width: 540px) {
    .invite {
      padding: 20px;
    }
  }
  @media (max-width: 400px) {
    .facts div {
      grid-template-columns: minmax(0, 1fr);
      gap: 4px;
    }
    .facts dd {
      justify-items: start;
      text-align: start;
    }
  }
</style>
