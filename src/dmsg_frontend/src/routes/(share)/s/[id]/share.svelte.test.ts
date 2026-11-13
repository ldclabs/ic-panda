import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { setLocale } from '$lib/i18n'
import {
  assemble,
  openManifest,
  unwrapKey,
  ShareError,
  type Descriptor,
  type ShareFile
} from '$lib/share/decrypt'
import SharePage from './+page.svelte'
import { readProfile, type ProfilePage } from '$lib/public-profile'

vi.mock('$lib/public-profile', () => ({ readProfile: vi.fn() }))
const sharer = '040g2081040g2081040g'
const sharerProfile: ProfilePage = {
  account: sharer,
  handle: 'alice',
  document: {
    id: `https://id.dmsg.net/${sharer}`,
    protocol: 'agent-delegation/1.0',
    controllers: [],
    updated_at: 1,
    name: 'Alice',
    avatar_url: 'https://assets.example.test/avatar.png'
  }
}

const network = vi.hoisted(() => ({
  authorize: vi.fn(),
  metadata: vi.fn(),
  bytes: vi.fn()
}))
vi.mock('$app/state', () => ({ page: { params: { id: 'a'.repeat(64) } } }))
vi.mock('$lib/share/download', () => ({
  authorizeMetadata: network.authorize,
  ShareDownload: class {
    metadata = network.metadata
    bytes = network.bytes
  }
}))
vi.mock('$lib/share/decrypt', async (original) => ({
  ...(await original<typeof import('$lib/share/decrypt')>()),
  openManifest: vi.fn(),
  unwrapKey: vi.fn(),
  assemble: vi.fn(),
  openChunk: vi.fn(async () => new Uint8Array([1]))
}))

let target: HTMLDivElement
let component: ReturnType<typeof mount> | undefined
let descriptor: Descriptor
let file: ShareFile
let save: ReturnType<typeof vi.spyOn>
const body =
  '# Project brief\n\nRead this **online**.\n\n![pixel](https://tracker.test/image)'

beforeEach(async () => {
  vi.clearAllMocks()
  vi.mocked(readProfile).mockResolvedValue(sharerProfile)
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value: function (this: HTMLDialogElement) {
      this.setAttribute('open', '')
    }
  })
  Object.defineProperty(HTMLDialogElement.prototype, 'close', {
    configurable: true,
    value: function (this: HTMLDialogElement) {
      this.removeAttribute('open')
      this.dispatchEvent(new Event('close'))
    }
  })
  await setLocale('en', false)
  history.replaceState(null, '', `/s/${'a'.repeat(64)}#${'A'.repeat(43)}`)
  descriptor = {
    status: 'available',
    share_id: 'a'.repeat(64),
    upload_id: 'b'.repeat(64),
    object_id: 'a'.repeat(64),
    version_id: 'c'.repeat(64),
    chunks: [{ digest: 'd'.repeat(64), size: 5 }],
    manifest: { digest: 'e'.repeat(64), size: 10 },
    expires_at: Date.now() + 3600000,
    max_downloads: 1,
    downloads: 0,
    password: false,
    key_wrap: null,
    created_at: Date.now()
  }
  file = {
    id: descriptor.object_id,
    version: descriptor.version_id,
    name: '项目 brief.md',
    mime: 'text/markdown',
    size: new TextEncoder().encode(body).length,
    sha256: 'f'.repeat(64),
    chunks: [{ id: 'chunk', size: 5, digest: 'd'.repeat(64) }],
    key: ''
  }
  network.metadata.mockResolvedValue(new Uint8Array([1]))
  network.authorize.mockResolvedValue({
    upload_id: descriptor.upload_id,
    expires_at: Date.now() + 50_000,
    manifest: { ...descriptor.manifest, url: 'https://example.test/manifest' }
  })
  network.bytes.mockResolvedValue(new Uint8Array([1]))
  vi.mocked(openManifest).mockImplementation(async () => file)
  vi.mocked(assemble).mockImplementation(async () =>
    new TextEncoder().encode(body)
  )
  vi.mocked(unwrapKey).mockResolvedValue(new Uint8Array(32))
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => ({ data: descriptor })
    }))
  )
  Object.defineProperty(URL, 'createObjectURL', {
    configurable: true,
    value: vi.fn(() => 'blob:test-preview')
  })
  Object.defineProperty(URL, 'revokeObjectURL', {
    configurable: true,
    value: vi.fn()
  })
  save = vi
    .spyOn(HTMLAnchorElement.prototype, 'click')
    .mockImplementation(() => {})
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = undefined
  target.remove()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

function start() {
  component = mount(SharePage, { target })
  flushSync()
}
async function shown(selector: string) {
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector(selector)).not.toBeNull()
  })
}
function click(selector: string) {
  target.querySelector<HTMLButtonElement>(selector)!.click()
  flushSync()
}

it('shows the invitation immediately and public facts while parallel metadata authorization is pending', async () => {
  let describe!: (response: unknown) => void
  let authorize!: (grant: unknown) => void
  vi.stubGlobal(
    'fetch',
    vi.fn(
      () =>
        new Promise((resolve) => {
          describe = resolve
        })
    )
  )
  network.authorize.mockReturnValue(
    new Promise((resolve) => {
      authorize = resolve
    })
  )
  start()
  expect(
    target.querySelector('[data-testid="share-invite"]')
  ).toHaveTextContent('Collect')
  expect(target.querySelector('[role="status"]')).toHaveTextContent('Loading')
  expect(target.querySelector('.facts')).toBeNull()
  expect(fetch).toHaveBeenCalledTimes(1)
  expect(network.authorize).toHaveBeenCalledTimes(1)

  describe({ ok: true, status: 200, json: async () => ({ data: descriptor }) })
  await shown('.facts')
  expect(target.querySelector('.facts')).toHaveTextContent('Expires')
  expect(target.querySelector('.facts')).toHaveTextContent('0 / 1')
  expect(target.querySelector('.facts')).not.toHaveTextContent(file.name)
  expect(target.querySelector('[role="status"]')).toHaveTextContent('Loading')
  expect(network.metadata).not.toHaveBeenCalled()
  expect(network.bytes).not.toHaveBeenCalled()

  const grant = {
    upload_id: descriptor.upload_id,
    expires_at: Date.now() + 50_000,
    manifest: descriptor.manifest
  }
  authorize(grant)
  await shown('[data-testid="share-preview"]')
  expect(network.metadata).toHaveBeenCalledExactlyOnceWith(grant)
  expect(target.querySelector('.facts')).toHaveTextContent(file.name)
  expect(network.bytes).not.toHaveBeenCalled()
})

it('settles an early authorization failure without replacing the link status or hiding the invitation', async () => {
  descriptor.status = 'revoked'
  network.authorize.mockRejectedValue(
    Object.assign(new Error('revoked'), { gone: 'revoked' })
  )
  start()
  await shown('[role="alert"]')
  expect(target.querySelector('[role="alert"]')).toHaveTextContent('revoked')
  expect(target.querySelector('[data-testid="share-invite"]')).not.toBeNull()
  expect(target.querySelector('.facts')).toBeNull()
  expect(network.metadata).not.toHaveBeenCalled()
})

it('shows the full name and type before content, reads inline, and saves without another fetch', async () => {
  start()
  await shown('[data-testid="share-preview"]')
  expect(target.querySelector('.facts')).toHaveTextContent(file.name)
  expect(target.querySelector('.facts')).toHaveTextContent('Markdown document')
  expect(target.querySelector('.facts')).not.toHaveTextContent('Extension')
  expect(network.metadata).toHaveBeenCalledTimes(1)
  expect(network.bytes).not.toHaveBeenCalled()
  expect(readProfile).not.toHaveBeenCalled()
  expect(target.querySelector('[data-testid="share-author"]')).toBeNull()
  click('[data-testid="share-preview"]')
  await shown('[data-testid="share-reader"]')
  expect(target.querySelector('.reader h1')).toHaveTextContent('Project brief')
  expect(target.querySelector('.reader img')).toBeNull()
  expect(save).not.toHaveBeenCalled()
  expect(network.bytes).toHaveBeenCalledExactlyOnceWith('chunks/0')
  click('button[aria-pressed]')
  expect(target.querySelector('.reader pre')).toHaveTextContent(
    '# Project brief'
  )
  click('[data-testid="share-save"]')
  click('[data-testid="share-save"]')
  expect(save).toHaveBeenCalledTimes(2)
  expect(network.bytes).toHaveBeenCalledTimes(1)
  expect(assemble).toHaveBeenCalledTimes(1)
  expect(URL.createObjectURL).toHaveBeenCalledTimes(2)
  expect(vi.mocked(URL.createObjectURL).mock.calls[0]![0]).toBe(
    vi.mocked(URL.createObjectURL).mock.calls[1]![0]
  )
  expect(
    target.querySelector('[data-testid="share-invite"]')
  ).toHaveTextContent('Plus')
})

it('keeps metadata behind the password and allows a retry after a wrong password', async () => {
  descriptor.password = true
  descriptor.key_wrap = 'wrapped'
  start()
  await shown('form.password')
  expect(network.metadata).not.toHaveBeenCalled()
  const input = target.querySelector<HTMLInputElement>('input')!
  input.value = 'wrong'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  flushSync()
  vi.mocked(unwrapKey).mockRejectedValueOnce(new ShareError('WRONG_PASSWORD'))
  target
    .querySelector('form')!
    .dispatchEvent(new Event('submit', { cancelable: true }))
  await shown('[role="alert"]')
  expect(network.metadata).not.toHaveBeenCalled()
  input.value = 'correct'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  flushSync()
  target
    .querySelector('form')!
    .dispatchEvent(new Event('submit', { cancelable: true }))
  await shown('[data-testid="share-preview"]')
  expect(network.metadata).toHaveBeenCalledTimes(1)
  expect(network.bytes).not.toHaveBeenCalled()
})

it('previews images without saving and releases their URL when leaving', async () => {
  file.name = 'photo.png'
  file.mime = 'image/png'
  start()
  await shown('[data-testid="share-preview"]')
  click('[data-testid="share-preview"]')
  await shown('[data-testid="share-image"]')
  expect(target.querySelector('.image-preview img')).toHaveAttribute(
    'src',
    'blob:test-preview'
  )
  expect(save).not.toHaveBeenCalled()
  await unmount(component!)
  component = undefined
  expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:test-preview')
})

it('never displays or saves content before successful full-file verification', async () => {
  vi.mocked(assemble).mockRejectedValue(new ShareError('INTEGRITY_FAILED'))
  start()
  await shown('[data-testid="share-preview"]')
  click('[data-testid="share-preview"]')
  await shown('[role="alert"]')
  expect(target.querySelector('.reader, .image-preview')).toBeNull()
  expect(URL.createObjectURL).not.toHaveBeenCalled()
  expect(save).not.toHaveBeenCalled()
})

it('discards a metadata response after the page has closed', async () => {
  let finish!: (file: ShareFile) => void
  vi.mocked(openManifest).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve
    })
  )
  start()
  await vi.waitFor(() => expect(openManifest).toHaveBeenCalledTimes(1))
  await unmount(component!)
  component = undefined
  finish(file)
  await Promise.resolve()
  flushSync()
  expect(target).toBeEmptyDOMElement()
  expect(network.bytes).not.toHaveBeenCalled()
})

it('offers download directly for unsupported content', async () => {
  file.mime = 'application/zip'
  file.name = 'archive.zip'
  start()
  await shown('[data-testid="share-download"]')
  expect(target.querySelector('[data-testid="share-preview"]')).toBeNull()
  click('[data-testid="share-download"]')
  await shown('[data-testid="share-save"]')
  expect(save).toHaveBeenCalledTimes(1)
  expect(network.bytes).toHaveBeenCalledTimes(1)
})

it('expands verified reading in a dialog, keeps its source view, and restores keyboard focus', async () => {
  start()
  await shown('[data-testid="share-preview"]')
  click('[data-testid="share-preview"]')
  await shown('[data-testid="share-reader"]')
  click('button[aria-pressed]')
  click('[data-testid="share-fullscreen"]')
  await shown('dialog[open]')
  expect(target.querySelectorAll('[data-testid="share-reader"]')).toHaveLength(
    1
  )
  expect(target.querySelector('dialog pre')).toHaveTextContent(
    '# Project brief'
  )
  click('[data-testid="share-exit-fullscreen"]')
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector('dialog')).not.toHaveAttribute('open')
    expect(
      target.querySelector('[data-testid="share-fullscreen"]')
    ).toHaveFocus()
  })
  expect(target.querySelector('.reader pre')).toHaveTextContent(
    '# Project brief'
  )
  expect(network.bytes).toHaveBeenCalledTimes(1)
})

it('loads an opted-in profile independently while reading and saving remain usable', async () => {
  descriptor.sharer_account = sharer
  let complete!: (profile: ProfilePage) => void
  vi.mocked(readProfile).mockReturnValue(
    new Promise((resolve) => {
      complete = resolve
    })
  )
  start()
  await shown('[data-testid="share-preview"]')
  expect(readProfile).toHaveBeenCalledTimes(1)
  expect(target.querySelector('[data-testid="share-profile"]')).toHaveAttribute(
    'href',
    `/${sharer}`
  )
  expect(
    target.querySelector('[data-testid="share-profile"]')
  ).toHaveTextContent('Check their public profile')
  click('[data-testid="share-preview"]')
  await shown('[data-testid="share-reader"]')
  click('[data-testid="share-save"]')
  expect(save).toHaveBeenCalledTimes(1)
  complete(sharerProfile)
  await vi.waitFor(() => {
    flushSync()
    expect(
      target.querySelector('[data-testid="share-profile"]')
    ).toHaveTextContent('Alice')
  })
  expect(
    target.querySelector('[data-testid="share-author"] img')
  ).toHaveAttribute('referrerpolicy', 'no-referrer')
  expect(
    target.querySelector('[data-testid="share-profile"]')
  ).toHaveTextContent('@alice')
  expect(network.bytes).toHaveBeenCalledTimes(1)
})

it('keeps the profile link and file actions when profile verification fails', async () => {
  descriptor.sharer_account = sharer
  vi.mocked(readProfile).mockRejectedValue(new Error('verification failed'))
  start()
  await shown('[data-testid="share-preview"]')
  expect(target.querySelector('[data-testid="share-profile"]')).toHaveAttribute(
    'href',
    `/${sharer}`
  )
  expect(
    target.querySelector('[data-testid="share-author"]')
  ).not.toHaveTextContent('Alice')
  expect(target.querySelector('[role="alert"]')).toBeNull()
  click('[data-testid="share-download"]')
  await shown('[data-testid="share-save"]')
  expect(save).toHaveBeenCalledTimes(1)
})

it('aborts profile work when leaving and ignores a late result', async () => {
  descriptor.sharer_account = sharer
  let complete!: (profile: ProfilePage) => void
  vi.mocked(readProfile).mockReturnValue(
    new Promise((resolve) => {
      complete = resolve
    })
  )
  start()
  await shown('[data-testid="share-preview"]')
  const signal = vi.mocked(readProfile).mock.calls[0]![1]
  await unmount(component!)
  component = undefined
  expect(signal.aborted).toBe(true)
  complete(sharerProfile)
  await Promise.resolve()
  flushSync()
  expect(target).toBeEmptyDOMElement()
})

it('ignores arbitrary profile targets and never resolves anonymous links', async () => {
  descriptor.sharer_account = 'https://example.test/fake'
  start()
  await shown('[data-testid="share-preview"]')
  expect(readProfile).not.toHaveBeenCalled()
  expect(target.querySelector('[data-testid="share-author"]')).toBeNull()
})
