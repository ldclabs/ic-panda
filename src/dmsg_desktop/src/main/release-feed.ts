/** Select dMsg releases independently of GitHub's publication order and other products in this repository. */
import { gt, valid } from 'semver'
import type { UpdateChannel } from '../shared/contract.ts'

export const RELEASES = 'https://github.com/ldclabs/ic-panda/releases'
const API = 'https://api.github.com/repos/ldclabs/ic-panda/releases'

/** New client tags are bare SemVer; beta.N is the only supported prerelease. */
export function releaseChannel(version: unknown): UpdateChannel | null {
  if (typeof version !== 'string' || valid(version) !== version) return null
  if (/^\d+\.\d+\.\d+$/.test(version)) return 'stable'
  return /^\d+\.\d+\.\d+-beta\.\d+$/.test(version) ? 'beta' : null
}

export interface DesktopRelease {
  version: string
  channel: UpdateChannel
  installer: string
  assets: string[]
}

/** Both the updater's package and the matching daemon must have finished uploading. */
export function desktopRelease(value: unknown, platform: string, arch: string): DesktopRelease | null {
  if (!value || typeof value !== 'object') return null
  const release = value as Record<string, unknown>
  const channel = releaseChannel(release.tag_name)
  if (!channel || release.draft !== false || release.prerelease !== (channel === 'beta')) return null
  const mac = platform === 'darwin' && arch === 'arm64'
  const win = platform === 'win32' && arch === 'x64'
  if (!mac && !win) return null
  const assets = Array.isArray(release.assets) ? release.assets.flatMap((asset) =>
    asset?.state === 'uploaded' && typeof asset.name === 'string' ? [asset.name] : []) : []
  const installer = mac ? 'dMsg-mac-arm64.zip' : 'dMsg-win-x64.exe'
  const runtime = mac ? 'dmsg-macos-arm64' : 'dmsg-windows-x64.exe'
  const feed = `${channel === 'beta' ? 'beta' : 'latest'}${mac ? '-mac' : ''}.yml`
  const required = [installer, `${installer}.sha256`, runtime, `${runtime}.sha256`, feed]
  if (!required.every((name) => assets.includes(name))) return null
  return { version: release.tag_name as string, channel, installer, assets }
}

export function selectRelease(releases: DesktopRelease[], channel: UpdateChannel, current: string): DesktopRelease | null {
  return releases.reduce<DesktopRelease | null>((best, release) => {
    if (channel === 'stable' && release.channel !== 'stable') return best
    return gt(release.version, best?.version ?? current) ? release : best
  }, null)
}

/** Short cache + ETags avoid repeated downloads; rate-limit responses delay retries without a bundled GitHub token. */
export class ReleaseFeed {
  private cache = new Map<string, { value: unknown; etag: string | null; until: number }>()
  private retryAt = 0
  private request: (url: string, init: RequestInit) => Promise<Response>

  constructor(request: (url: string, init: RequestInit) => Promise<Response> = fetch) {
    this.request = request
  }

  private async read(url: string, signal: AbortSignal): Promise<unknown> {
    const now = Date.now()
    const cached = this.cache.get(url)
    if (cached && cached.until > now) return cached.value
    if (now < this.retryAt) throw new Error('GitHub rate limit; please check for updates again later.')
    const response = await this.request(url, {
      headers: { Accept: 'application/vnd.github+json', ...(cached?.etag ? { 'If-None-Match': cached.etag } : {}) },
      signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)])
    })
    if (response.status === 304 && cached) {
      cached.until = now + 60_000
      return cached.value
    }
    if (response.status === 403 || response.status === 429) {
      const reset = Number(response.headers.get('x-ratelimit-reset')) * 1000
      const retry = Number(response.headers.get('retry-after')) * 1000
      this.retryAt = Math.max(now + 60_000, reset || 0, now + (retry || 0))
    }
    if (!response.ok) throw new Error(`GitHub update check failed (HTTP ${response.status}).`)
    const value: unknown = await response.json()
    this.cache.set(url, { value, etag: response.headers.get('etag'), until: now + 60_000 })
    return value
  }

  async latest(channel: UpdateChannel, current: string, platform: string, arch: string, signal: AbortSignal): Promise<DesktopRelease | null> {
    if (channel === 'stable') {
      const release = desktopRelease(await this.read(`${API}/latest`, signal), platform, arch)
      if (!release || release.channel !== 'stable') throw new Error('The latest release has no complete stable dMsg update for this platform.')
      return selectRelease([release], channel, current)
    }
    const candidates: DesktopRelease[] = []
    for (let page = 1; ; page++) {
      signal.throwIfAborted()
      const releases = await this.read(`${API}?per_page=100&page=${page}`, signal)
      if (!Array.isArray(releases)) throw new Error('Invalid GitHub releases response.')
      for (const value of releases) {
        const release = desktopRelease(value, platform, arch)
        if (release) candidates.push(release)
      }
      if (releases.length < 100) break
    }
    return selectRelease(candidates, channel, current)
  }
}
