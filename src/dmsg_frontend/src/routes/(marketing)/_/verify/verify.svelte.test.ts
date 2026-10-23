import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { setLocale } from '$lib/i18n'
import {
  readBundle,
  verifyBundle,
  VerifyError,
  type BundleReport
} from '$lib/verify'
import VerifyPage from './+page.svelte'

vi.mock('$lib/verify', () => ({
  readBundle: vi.fn(),
  verifyBundle: vi.fn(),
  VerifyError: class extends Error {
    constructor(readonly reason: string) {
      super(reason)
    }
  }
}))

const account = 'dba38b6u97gecq800000'
const result: BundleReport = {
  valid: true,
  failed: [],
  content: 'digest_only',
  signature: true,
  binding: true,
  evidence: true,
  account,
  home: 'wqqa6-3aaaa-aaaaj-qsjvq-cai',
  keyFingerprint: new Uint8Array(32).fill(42),
  approvedAt: 1792736130000n
}
let target: HTMLDivElement
let component: ReturnType<typeof mount>

beforeEach(async () => {
  await setLocale('en', false)
  target = document.createElement('div')
  document.body.append(target)
  vi.mocked(verifyBundle).mockResolvedValue(result)
  component = mount(VerifyPage, { target })
  flushSync()
})

afterEach(async () => {
  await unmount(component)
  target.remove()
  vi.resetAllMocks()
})

function pick(kind: 'bundle' | 'original', name = 'evidence.json') {
  const file = new File(['test'], name)
  // jsdom File does not implement the browser's text/arrayBuffer methods.
  Object.assign(file, {
    text: async () => 'test',
    arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer
  })
  const input = target.querySelector<HTMLInputElement>(
    `[data-testid="verify-${kind}"]`
  )!
  Object.defineProperty(input, 'files', { configurable: true, value: [file] })
  input.dispatchEvent(new Event('change', { bubbles: true }))
  flushSync()
}

async function verify() {
  target
    .querySelector('form')!
    .dispatchEvent(new Event('submit', { cancelable: true }))
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector('form')).toHaveAttribute('aria-busy', 'false')
  })
}

it('links the verified signer to the permanent profile and marks an absent original as unchecked', async () => {
  expect(target.querySelector('button[type="submit"]')).toBeDisabled()
  pick('bundle')
  await verify()
  const profile = target.querySelector(`a[href="/${account}"]`)
  expect(profile).toHaveTextContent(account)
  expect(profile).toHaveTextContent('View profile')
  expect(profile).toHaveAttribute('target', '_blank')
  expect(target.querySelector('.checks .pending')).toHaveTextContent(
    'Not checked'
  )
  expect(target.querySelector('.checks .pending')).toHaveTextContent(
    'add the original file'
  )
  expect(target.querySelector('details')).not.toHaveAttribute('open')
  expect(target.querySelector('details')).toHaveTextContent('2a'.repeat(32))
})

it.each(['signature', 'binding', 'evidence'] as const)(
  'does not present an unverified %s as a signer profile',
  async (check) => {
    vi.mocked(verifyBundle).mockResolvedValue({
      ...result,
      valid: false,
      [check]: false
    })
    pick('bundle')
    await verify()
    expect(target.querySelector('.signer-link')).toBeNull()
    expect(target.querySelector('h2')).toHaveTextContent(
      'The bundle does not verify'
    )
    expect(target.querySelector('.failed')).toHaveTextContent('Failed')
    expect(target.querySelector('details')).toHaveTextContent(account)
  }
)

it('clears stale results when either file changes or the optional file is removed', async () => {
  pick('bundle')
  await verify()
  pick('original', 'original.txt')
  expect(target.querySelector('[data-testid="verify-report"]')).toBeNull()
  await verify()
  expect(verifyBundle).toHaveBeenLastCalledWith(
    undefined,
    new Uint8Array([1, 2, 3])
  )
  target.querySelector<HTMLButtonElement>('.remove')!.click()
  flushSync()
  expect(target.querySelector('[data-testid="verify-report"]')).toBeNull()
  await verify()
  expect(verifyBundle).toHaveBeenLastCalledWith(undefined, undefined)
  pick('bundle', 'another.json')
  expect(target.querySelector('[data-testid="verify-report"]')).toBeNull()
})

it('keeps a verified signer visible while clearly reporting a mismatched original', async () => {
  vi.mocked(verifyBundle).mockResolvedValue({
    ...result,
    valid: false,
    content: 'mismatch',
    failed: ['content']
  })
  pick('bundle')
  pick('original')
  await verify()
  expect(target.querySelector('.signer-link')).not.toBeNull()
  expect(target.querySelector('.checks .failed')).toHaveTextContent(
    'The file does not match'
  )
  expect(target.querySelector('h2')).toHaveTextContent(
    'The bundle does not verify'
  )
})

it('prevents changes and duplicate submissions while verification is running', async () => {
  let resolve!: (report: BundleReport) => void
  vi.mocked(verifyBundle).mockReturnValue(
    new Promise((done) => {
      resolve = done
    })
  )
  pick('bundle')
  const form = target.querySelector('form')!
  form.dispatchEvent(new Event('submit', { cancelable: true }))
  await vi.waitFor(() => expect(verifyBundle).toHaveBeenCalledTimes(1))
  flushSync()
  expect(target.querySelector('[data-testid="verify-bundle"]')).toBeDisabled()
  expect(target.querySelector('[data-testid="verify-original"]')).toBeDisabled()
  expect(target.querySelector('button[type="submit"]')).toBeDisabled()
  form.dispatchEvent(new Event('submit', { cancelable: true }))
  resolve(result)
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector('[data-testid="verify-report"]')).not.toBeNull()
  })
  expect(verifyBundle).toHaveBeenCalledTimes(1)
})

it('localizes errors after a language change and allows retrying', async () => {
  vi.mocked(readBundle).mockImplementationOnce(() => {
    throw new VerifyError('not_bundle')
  })
  pick('bundle')
  await verify()
  expect(target.querySelector('[role="alert"]')).toHaveTextContent(
    'This is not a dMsg attestation bundle.'
  )
  await setLocale('zh', false)
  flushSync()
  expect(target.querySelector('[role="alert"]')).toHaveTextContent(
    '这不是 dMsg 证据包。'
  )
  await verify()
  expect(target.querySelector('[role="alert"]')).toBeNull()
  expect(target.querySelector('.signer-link')).toHaveTextContent('查看个人主页')
})
