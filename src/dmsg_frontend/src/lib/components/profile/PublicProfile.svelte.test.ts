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
  avatar: null,
  published: {
    profile: {
      version: 1,
      prev_hash: null,
      display_name: 'Alice',
      bio: '<script>alert(1)</script>',
      avatar_upload: null,
      links: ['https://example.com/']
    },
    hash: 'aa'.repeat(32),
    deviceId: '07'.repeat(32),
    deviceActive: true,
    certifiedAt: 1n
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

it('displays verified identity, escaped content and a permanent account link', async () => {
  render()
  await settled()
  expect(target.querySelector('h1')?.textContent).toBe('Alice')
  expect(target.textContent).toContain('Device signature verified')
  expect(target.textContent).toContain('does not prove the latest version')
  expect(target.textContent).toContain('<script>alert(1)</script>')
  expect(target.querySelector('script')).toBeNull()
  expect(target.querySelector(`a[href="/${account}"]`)).not.toBeNull()
  expect(
    document.querySelector('link[rel="canonical"]')?.getAttribute('href')
  ).toContain(`/${account}`)
  expect(
    target.querySelector('a[href="https://example.com/"]')?.getAttribute('rel')
  ).toBe('noopener noreferrer')
})

it('separates a revoked historical signature from an active device', async () => {
  vi.mocked(readProfile).mockResolvedValue({
    ...result,
    published: { ...result.published!, deviceActive: false }
  })
  render()
  await settled()
  expect(target.textContent).toContain('Device signature verified')
  expect(target.textContent).toContain('Revoked · historical signature')
})

it('shows an existing account without a profile without claiming a verified signature', async () => {
  vi.mocked(readProfile).mockResolvedValue({ ...result, published: null })
  render()
  await settled()
  expect(target.textContent).toContain('has not published a public profile')
  expect(target.textContent).not.toContain('Device signature verified')
})

it('hides failed verification and retries explicitly', async () => {
  vi.mocked(readProfile).mockRejectedValueOnce(
    new ProfileError('verification_failed')
  )
  render()
  await settled()
  expect(target.querySelector('[role="alert"]')?.textContent).toContain(
    'could not be verified'
  )
  expect(target.textContent).not.toContain('Alice')
  target.querySelector<HTMLButtonElement>('.status button')!.click()
  flushSync()
  await settled()
  expect(target.textContent).toContain('Alice')
})

it('a late old request cannot overwrite a new route; navigation cancels its signal', async () => {
  let resolveOld!: (value: ProfilePage) => void
  vi.mocked(readProfile).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveOld = resolve
      })
  )
  const props = render('alice')
  const oldSignal = vi.mocked(readProfile).mock.calls[0]![1]
  props.identifier = account
  flushSync()
  await settled()
  expect(oldSignal.aborted).toBe(true)
  resolveOld({
    ...result,
    published: {
      ...result.published!,
      profile: { ...result.published!.profile, display_name: 'Stale result' }
    }
  })
  await Promise.resolve()
  flushSync()
  expect(target.textContent).not.toContain('Stale result')
  expect(target.textContent).toContain('Alice')
})
