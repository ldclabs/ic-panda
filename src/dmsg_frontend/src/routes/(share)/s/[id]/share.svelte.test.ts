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

const network = vi.hoisted(() => ({ metadata: vi.fn(), bytes: vi.fn() }))
vi.mock('$app/state', () => ({ page: { params: { id: 'a'.repeat(64) } } }))
vi.mock('$lib/share/download', () => ({
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

it('shows the full name and type before content, reads inline, and saves without another fetch', async () => {
  start()
  await shown('[data-testid="share-preview"]')
  expect(target.querySelector('.facts')).toHaveTextContent(file.name)
  expect(target.querySelector('.facts')).toHaveTextContent('Markdown document')
  expect(target.querySelector('.facts')).not.toHaveTextContent('Extension')
  expect(network.metadata).toHaveBeenCalledTimes(1)
  expect(network.bytes).not.toHaveBeenCalled()
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
