import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { setLocale } from '$lib/i18n'
import {
  readProfile,
  ProfileError,
  type ProfilePage
} from '$lib/public-profile'
import PublicProfile from './PublicProfile.svelte'
vi.mock('$lib/public-profile', () => ({
  readProfile: vi.fn(),
  ProfileError: class extends Error {
    constructor(readonly reason: string) {
      super(reason)
    }
  }
}))
const account = '040g2081040g2081040g'
const result: ProfilePage = {
  account,
  handle: 'alice',
  document: {
    id: `https://id.dmsg.net/${account}`,
    protocol: 'agent-delegation/1.0',
    controllers: [],
    updated_at: 1,
    name: 'Alice',
    description: '<script>alert(1)</script>',
    links: [{ name: 'Website', url: 'https://example.com/', rel: 'related' }],
    extra: {
      'dmsg.link_images': {
        'https://example.com/': 'https://example.com/logo.png'
      }
    }
  }
}
let target: HTMLDivElement
let component: ReturnType<typeof mount> | undefined
beforeEach(async () => {
  await setLocale('en', false)
  target = document.createElement('div')
  document.body.append(target)
  vi.mocked(readProfile).mockResolvedValue(result)
})
afterEach(async () => {
  if (component) await unmount(component)
  component = undefined
  target.remove()
  vi.resetAllMocks()
})
const render = (identifier = 'alice') => {
  const props = $state({ identifier })
  component = mount(PublicProfile, { target, props })
  flushSync()
  return props
}
const settled = async () => {
  await vi.waitFor(() => {
    flushSync()
    expect(target.querySelector('[aria-busy]')?.getAttribute('aria-busy')).toBe(
      'false'
    )
  })
}
it('renders the official document and legacy link artwork, escaping text', async () => {
  render()
  await settled()
  expect(target.querySelector('h1')?.textContent).toBe('Alice')
  expect(target.textContent).toContain('Certified principal document')
  expect(target.textContent).toContain('<script>alert(1)</script>')
  expect(target.querySelector('script')).toBeNull()
  expect(
    target.querySelector('a[href="https://example.com/"]')?.textContent
  ).toContain('Website')
  expect(target.querySelector('.link-image')?.getAttribute('src')).toBe(
    'https://example.com/logo.png'
  )
  expect(
    target.querySelector(`a[href="https://id.dmsg.net/${account}"]`)
  ).not.toBeNull()
})
it('distinguishes an unpublished document', async () => {
  vi.mocked(readProfile).mockResolvedValue({ ...result, document: null })
  render()
  await settled()
  expect(target.textContent).toContain('has not published')
  expect(target.textContent).not.toContain('Certified principal document')
})
it('keeps handle casing and links only explicit HTTP URLs in plain description text', async () => {
  vi.mocked(readProfile).mockResolvedValue({
    ...result,
    handle: 'ICPandaDAO',
    document: {
      ...result.document!,
      description:
        'See https://Anda.AI | http://dMsg.net.\n<script>alert(1)</script> javascript:alert(1) plain.example'
    }
  })
  render('ICPandaDAO')
  await settled()
  expect(target.querySelector('.handle')?.textContent).toBe('@ICPandaDAO')
  const bio = target.querySelector('.bio')!
  const links = [...bio.querySelectorAll('a')]
  expect(links.map((link) => link.getAttribute('href'))).toEqual([
    'https://Anda.AI',
    'http://dMsg.net'
  ])
  expect(links.map((link) => link.textContent)).toEqual([
    'https://Anda.AI',
    'http://dMsg.net'
  ])
  expect(
    links.every(
      (link) => link.target === '_blank' && link.rel === 'noopener noreferrer'
    )
  ).toBe(true)
  expect(bio.textContent).toBe(
    'See https://Anda.AI | http://dMsg.net.\n<script>alert(1)</script> javascript:alert(1) plain.example'
  )
  expect(bio.querySelector('script')).toBeNull()
})
it('hides failed verification and retries explicitly', async () => {
  vi.mocked(readProfile).mockRejectedValueOnce(
    new ProfileError('verification_failed')
  )
  render()
  await settled()
  expect(target.textContent).not.toContain('Alice')
  target.querySelector<HTMLButtonElement>('.status button')!.click()
  flushSync()
  await settled()
  expect(target.textContent).toContain('Alice')
  expect(vi.mocked(readProfile).mock.calls[1]?.[2]).toEqual({ refresh: true })
})
it('late responses cannot overwrite navigation', async () => {
  let resolveOld!: (v: ProfilePage) => void
  vi.mocked(readProfile).mockImplementationOnce(
    () => new Promise((r) => (resolveOld = r))
  )
  const props = render()
  const signal = vi.mocked(readProfile).mock.calls[0]![1]
  props.identifier = account
  flushSync()
  await settled()
  expect(signal.aborted).toBe(true)
  resolveOld({ ...result, document: { ...result.document!, name: 'Stale' } })
  await Promise.resolve()
  flushSync()
  expect(target.textContent).not.toContain('Stale')
})
it('offers the legacy page only for reserved names', async () => {
  vi.mocked(readProfile).mockRejectedValueOnce(
    new ProfileError('legacy_reserved')
  )
  render('PANDA')
  await settled()
  expect(
    target.querySelector('a[href="/legacy/users/panda"]')?.textContent
  ).toBe('View legacy profile')
  expect(target.textContent).not.toContain('Certified principal document')
})

it('renders the page shell while background verification is pending and never shows rejected data', async () => {
  let reject!: (error: ProfileError) => void
  vi.mocked(readProfile).mockImplementationOnce(
    () =>
      new Promise((_resolve, fail) => {
        reject = fail
      })
  )
  render()
  expect(target.querySelector('.site-header')).not.toBeNull()
  expect(target.querySelector('[role="status"]')?.textContent).toContain(
    'Loading and verifying profile'
  )
  expect(target.querySelector('.identity')).toBeNull()
  expect(target.textContent).not.toContain('Certified principal document')
  reject(new ProfileError('verification_failed'))
  await settled()
  expect(target.querySelector('[role="alert"]')).not.toBeNull()
  expect(target.querySelector('.identity')).toBeNull()
})
