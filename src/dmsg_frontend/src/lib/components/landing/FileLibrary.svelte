<script lang="ts">
  import { onMount } from 'svelte'
  import { t, tr } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import type { GlyphName } from './glyphs'
  import { prefersReducedMotion } from './motion'

  // The rules of `dmsg files`: the library lives on this device, only Back up
  // uploads, the relay gets random IDs and ciphertext, and a deletion on one
  // side never reaches the other. Sample files; nothing leaves the page.
  type File = {
    id: string
    name: string
    folder: string
    bytes: number
    versions: number
    glyph: GlyphName
    trashed?: boolean
    backup?: string | undefined
    progress?: number | undefined
  }
  type Copy = {
    id: string
    file: File
    bytes: number
    device: string
    at: string
  }

  const KiB = 1024
  const MiB = 1024 * KiB
  const GiB = 1024 * MiB
  const ROOM = 100 * MiB
  const LIMIT = 100 * MiB
  const HERE = 'MacBook Air'
  const OTHER = 'Mac mini'
  const DAYS = 30

  // One version encrypted in 1 MiB chunks, each with its tag, plus a manifest.
  const sealed = (bytes: number) => bytes + Math.ceil(bytes / MiB) * 44 + 412
  const upload = (seed: string) => `upl_${hex(seed)}`

  function hex(seed: string): string {
    let h = 2166136261
    for (const ch of seed) h = Math.imul(h ^ ch.charCodeAt(0), 16777619)
    return (h >>> 0).toString(16).padStart(8, '0')
  }

  // Isolated left to right, so a size keeps its order inside Arabic text.
  function size(bytes: number): string {
    const one = (n: number) => String(Number(n.toFixed(1)))
    const text =
      bytes >= GiB
        ? `${one(bytes / GiB)} GiB`
        : bytes >= MiB
          ? `${one(bytes / MiB)} MiB`
          : `${Math.round(bytes / KiB)} KiB`
    return String.fromCharCode(0x2066) + text + String.fromCharCode(0x2069)
  }

  function stamp(): string {
    const d = new Date()
    const p = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
  }

  const photo: File = {
    id: 'fil_3',
    name: 'team-photo.jpg',
    folder: 'Photos',
    bytes: 6.4 * MiB,
    versions: 1,
    glyph: 'image'
  }
  // Backed up from the other device; this one learns its name only by fetching.
  const tax: File = {
    id: 'fil_5',
    name: 'tax-2025.pdf',
    folder: 'Documents',
    bytes: 1.2 * MiB,
    versions: 1,
    glyph: 'file'
  }

  let files = $state<File[]>([
    {
      id: 'fil_1',
      name: 'passport-scan.pdf',
      folder: 'Documents',
      bytes: 2.3 * MiB,
      versions: 1,
      glyph: 'file'
    },
    {
      id: 'fil_2',
      name: 'lease-2026.pdf',
      folder: 'Contracts',
      bytes: 860 * KiB,
      versions: 3,
      glyph: 'file'
    },
    { ...photo, backup: upload(photo.id) },
    {
      id: 'fil_4',
      name: 'launch-film.mov',
      folder: 'Videos',
      bytes: 1.9 * GiB,
      versions: 1,
      glyph: 'file'
    }
  ])
  let copies = $state<Copy[]>([
    {
      id: upload(photo.id),
      file: photo,
      bytes: sealed(photo.bytes),
      device: HERE,
      at: '2026-10-14 09:12'
    },
    {
      id: upload(tax.id),
      file: tax,
      bytes: sealed(tax.bytes),
      device: OTHER,
      at: '2026-10-11 18:40'
    }
  ])
  let asking = $state('')
  let armed = $state('')
  let flash = $state('')
  let notice = $state('')
  let started = $state(0)
  let armTimer: ReturnType<typeof setTimeout> | undefined
  let flashTimer: ReturnType<typeof setTimeout> | undefined
  let timers: ReturnType<typeof setInterval>[] = []

  const shelf = $derived(files.filter((file) => !file.trashed))
  const trash = $derived(files.filter((file) => file.trashed))
  const local = $derived(shelf.reduce((sum, file) => sum + file.bytes, 0))
  const used = $derived(copies.reduce((sum, copy) => sum + copy.bytes, 0))
  const statusLabel = $derived<Record<ReturnType<typeof status>, string>>({
    local: $t('Only on this device'),
    up: $t('Backing up…'),
    backed: $t('Backed up'),
    large: $t('Over the cloud’s 100 MiB limit')
  })

  const status = (file: File) =>
    file.progress !== undefined
      ? 'up'
      : file.backup
        ? 'backed'
        : file.bytes > LIMIT
          ? 'large'
          : 'local'
  const meta = (file: File) =>
    [
      $t(file.folder),
      size(file.bytes),
      file.versions > 1 ? $t('{count} versions', { count: file.versions }) : ''
    ]
      .filter(Boolean)
      .join(' · ')
  const owner = (copy: Copy) => files.find((file) => file.id === copy.file.id)

  function say(text: string, copy?: string) {
    notice = text
    flash = copy ?? ''
    clearTimeout(flashTimer)
    flashTimer = setTimeout(() => (flash = ''), 1600)
  }

  function backUp(file: File) {
    asking = ''
    file.progress = 0
    const step = prefersReducedMotion() ? 0.5 : 0.12
    const timer = setInterval(() => {
      file.progress = Math.min((file.progress ?? 0) + step, 1)
      if (file.progress < 1) return
      clearInterval(timer)
      const id = upload(`${file.id}${Date.now()}`)
      copies.unshift({
        id,
        file,
        bytes: sealed(file.bytes),
        device: HERE,
        at: stamp()
      })
      file.backup = id
      file.progress = undefined
      started++
      say(
        tr('Backed up. The relay got ciphertext under a random ID, no name.'),
        id
      )
    }, 220)
    timers.push(timer)
  }

  function toTrash(file: File) {
    file.trashed = true
    asking = ''
    say(
      file.backup
        ? tr('In the trash here. Its cloud copy stays as it was.')
        : tr('In the trash here, kept {days} days.', { days: DAYS }),
      file.backup
    )
  }

  function restore(file: File) {
    file.trashed = false
    say(tr('Restored on this device.'))
  }

  // Deleting a cloud copy is final, so it takes a second click.
  function remove(copy: Copy) {
    if (armed !== copy.id) {
      armed = copy.id
      clearTimeout(armTimer)
      armTimer = setTimeout(() => (armed = ''), 3000)
      return
    }
    armed = ''
    copies = copies.filter((c) => c.id !== copy.id)
    const file = owner(copy)
    if (file) file.backup = undefined
    say(
      file
        ? tr('Cloud copy deleted. The file on this device is unchanged.')
        : tr('Cloud copy deleted, and its room is freed.')
    )
  }

  function fetchCopy(copy: Copy) {
    files.push({ ...copy.file, backup: copy.id })
    say(
      tr(
        'Fetched and checked. Only on your devices does it have a name: {name}.',
        { name: copy.file.name }
      )
    )
  }

  onMount(() => () => {
    timers.forEach(clearInterval)
    clearTimeout(armTimer)
    clearTimeout(flashTimer)
  })
</script>

<div class="files-frame">
  <div class="files-lab">
    <section class="fl-pane" aria-labelledby="fl-device">
      <header class="fl-head">
        <span class="fl-title"
          ><Glyph name="laptop" size={16} /><strong id="fl-device"
            >{$t('On your device')}</strong
          ><code>{HERE}</code></span
        >
        <small
          >{$t('{count} files · {size}', {
            count: shelf.length,
            size: size(local)
          })}</small
        >
      </header>
      <ul class="fl-list">
        {#each shelf as file (file.id)}
          {@const mark = status(file)}
          <li class="fl-row">
            <span class="vi-icon"><Glyph name={file.glyph} size={16} /></span>
            <span class="fl-text">
              <strong>{file.name}</strong>
              <small>{meta(file)}</small>
            </span>
            <span class="fl-state {mark}"
              ><i aria-hidden="true"></i>{mark === 'up'
                ? `${statusLabel.up} ${Math.round((file.progress ?? 0) * 100)}%`
                : statusLabel[mark]}</span
            >
            <span class="fl-actions">
              {#if mark === 'local'}
                <button
                  class="chip-btn"
                  aria-expanded={asking === file.id}
                  onclick={() => (asking = asking === file.id ? '' : file.id)}
                  ><Glyph name="cloud-up" size={14} />{$t('Back up')}</button
                >
              {/if}
              <button
                class="chip-btn icon"
                disabled={mark === 'up'}
                onclick={() => toTrash(file)}
                aria-label={$t('Move {name} to the trash', { name: file.name })}
                ><Glyph name="trash" size={14} /></button
              >
            </span>
            {#if mark === 'up'}
              <span
                class="fl-bar"
                style:--p={file.progress ?? 0}
                aria-hidden="true"
              ></span>
            {/if}
            {#if asking === file.id}
              <div class="fl-ask" role="group" aria-label={$t('Back up')}>
                <p>
                  <strong>{$t('Back up to the cloud?')}</strong>
                  {$t(
                    '{size}, encrypted as it is. Nothing is uploaded without this step, and nothing is deleted on either side.',
                    { size: size(sealed(file.bytes)) }
                  )}
                </p>
                <p class="fl-room">
                  {$t('Cloud room: {used} used of {limit}', {
                    used: size(used),
                    limit: size(ROOM)
                  })}
                </p>
                <div class="fl-ask-actions">
                  <button class="button secondary" onclick={() => (asking = '')}
                    >{$t('Cancel')}</button
                  >
                  <button class="button primary" onclick={() => backUp(file)}
                    >{$t('Back up')}</button
                  >
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if trash.length}
        <p class="fl-sub">
          <Glyph name="trash" size={14} />{$t(
            'Trash · kept {days} days. Cloud copies are not affected.',
            { days: DAYS }
          )}
        </p>
        <ul class="fl-list">
          {#each trash as file (file.id)}
            <li class="fl-row trashed">
              <span class="vi-icon"><Glyph name={file.glyph} size={16} /></span>
              <span class="fl-text">
                <strong>{file.name}</strong>
                <small>{$t(file.folder)} · {size(file.bytes)}</small>
              </span>
              <span class="fl-actions">
                <button class="chip-btn" onclick={() => restore(file)}
                  ><Glyph name="restore" size={14} />{$t('Restore')}</button
                >
              </span>
            </li>
          {/each}
        </ul>
      {/if}
      <p class="fl-foot">
        {$t(
          'Up to 4 GiB a file. Names, folders, tags, notes and every version are encrypted here.'
        )}
      </p>
    </section>

    <section class="fl-pane cloud" aria-labelledby="fl-cloud">
      <header class="fl-head">
        <span class="fl-title"
          ><Glyph name="cloud" size={16} /><strong id="fl-cloud"
            >{$t('In the dMsg cloud')}</strong
          ></span
        >
        <small
          >{$t('{used} of {limit}', {
            used: size(used),
            limit: size(ROOM)
          })}</small
        >
      </header>
      <span class="fl-meter" aria-hidden="true"
        ><i style:width="{Math.min((used / ROOM) * 100, 100)}%"></i></span
      >
      <ul class="fl-list">
        {#each copies as copy (copy.id)}
          {@const file = owner(copy)}
          <li class="fl-row" class:flash={flash === copy.id}>
            <span class="vi-icon sealed"><Glyph name="lock" size={16} /></span>
            <span class="fl-text">
              <code>{copy.id}</code>
              <small dir="ltr"
                >{size(copy.bytes)} · {copy.device} · {copy.at}</small
              >
            </span>
            <span class="fl-actions">
              {#if !file}
                <button class="chip-btn" onclick={() => fetchCopy(copy)}
                  ><Glyph name="cloud-down" size={14} />{$t(
                    'Fetch into the library'
                  )}</button
                >
              {/if}
              <button
                class="chip-btn danger"
                class:armed={armed === copy.id}
                onclick={() => remove(copy)}
                ><Glyph name="trash" size={14} />{armed === copy.id
                  ? $t('Click again to delete for good')
                  : $t('Delete the cloud copy')}</button
              >
            </span>
          </li>
        {:else}
          <li class="fl-empty">
            {$t('Empty. Nothing arrives here until you back it up.')}
          </li>
        {/each}
      </ul>
      <p class="fl-foot">
        {$t(
          'The relay sees sizes, times and devices. Names, folders and types stay sealed.'
        )}
      </p>
    </section>
  </div>

  <div class="fl-dock">
    <p class="fl-notice" aria-live="polite">
      {notice || $t('Back up a file, delete one side, then look at the other.')}
    </p>
    <dl class="gate-stats">
      <div>
        <dt>{$t('Backups you started')}</dt>
        <dd>{started}</dd>
      </div>
      <div class="zero">
        <dt>{$t('Uploaded without you')}</dt>
        <dd>0</dd>
      </div>
      <div class="zero">
        <dt>{$t('Deletions that crossed over')}</dt>
        <dd>0</dd>
      </div>
    </dl>
  </div>
</div>
