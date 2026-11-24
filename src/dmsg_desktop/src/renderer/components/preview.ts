/**
 * Pure fullscreen preview classification, filtering, focus preservation and
 * zoom math shared by the Files and Channels lightboxes (and tested under node).
 */
import type { PreviewClass, PreviewSource } from '../../shared/contract'

export const MAX_PREVIEW_IMAGE_BYTES = 20 * 1024 * 1024
export const MAX_PREVIEW_MARKDOWN_BYTES = 2 * 1024 * 1024
export const MAX_PREVIEW_TEXT_BYTES = 1024 * 1024

export const ZOOM_MIN = 0.25
export const ZOOM_MAX = 4.0
export const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.85, 1, 1.25, 1.5, 2, 3, 4] as const

export type PreviewFilter = 'all' | 'previewable' | 'images' | 'documents'
export const PREVIEW_FILTERS: readonly PreviewFilter[] = ['all', 'previewable', 'images', 'documents']

export interface PreviewItem {
  key: string
  source: PreviewSource
  name: string
  mime: string
  size: number
  updatedAt?: number
  sha256?: string
  status?: string
  versionLabel?: string
  previewClass?: PreviewClass
  previewable?: boolean
  placeholderUrl?: string
}

const IMAGE_MIMES = new Set([
  'image/png',
  'image/jpeg',
  'image/jpg',
  'image/gif',
  'image/webp',
  'image/bmp',
  'image/avif',
  'image/heic',
  'image/heif'
])

const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'avif', 'heic', 'heif'])
const MARKDOWN_EXTS = new Set(['md', 'markdown', 'mdown', 'mkd'])

const TEXT_MIMES = new Set([
  'application/json',
  'application/ld+json',
  'application/xml',
  'application/toml',
  'application/yaml',
  'application/x-yaml',
  'application/javascript',
  'application/x-javascript',
  'application/typescript',
  'application/x-sh',
  'application/sql',
  'application/csv'
])

const TEXT_EXTS = new Set([
  'txt', 'log', 'json', 'jsonc', 'yaml', 'yml', 'toml', 'xml', 'csv', 'tsv',
  'ini', 'cfg', 'conf', 'env', 'sh', 'bash', 'zsh', 'py', 'rs', 'js', 'mjs',
  'cjs', 'ts', 'mts', 'cts', 'jsx', 'tsx', 'html', 'htm', 'css', 'scss', 'sql',
  'swift', 'kt', 'kts', 'java', 'go', 'c', 'h', 'cpp', 'cc', 'hpp', 'rb', 'php',
  'svelte', 'vue', 'dockerfile', 'makefile'
])

function extensionOf(name: string | undefined): string {
  if (!name) return ''
  const dot = name.lastIndexOf('.')
  if (dot < 0 || dot === name.length - 1) return ''
  return name.slice(dot + 1).toLowerCase()
}

function normalizedMime(mime: string | undefined): string {
  return (mime ?? '').split(';', 1)[0].trim().toLowerCase()
}

/** Resolve preview class from MIME type and filename, mirroring `dmsg_client::preview::classify`. */
export function classifyPreview(mime: string | undefined, name: string | undefined): PreviewClass {
  const m = normalizedMime(mime)
  const ext = extensionOf(name)
  if (m === 'application/vnd.dmsg.secret-package' || m === 'image/svg+xml' || ext === 'svg' || ext === 'svgz') {
    return 'unsupported'
  }
  const generic = m === '' || m === 'application/octet-stream'
  if (m === 'text/markdown' || m === 'text/x-markdown' || ((generic || m === 'text/plain') && MARKDOWN_EXTS.has(ext))) {
    return 'markdown'
  }
  if (IMAGE_MIMES.has(m) || (generic && IMAGE_EXTS.has(ext))) {
    return 'image'
  }
  if (m.startsWith('text/') || TEXT_MIMES.has(m) || (generic && TEXT_EXTS.has(ext))) {
    return 'text'
  }
  return 'unsupported'
}

export function previewSizeLimit(kind: PreviewClass): number {
  if (kind === 'image') return MAX_PREVIEW_IMAGE_BYTES
  if (kind === 'markdown') return MAX_PREVIEW_MARKDOWN_BYTES
  if (kind === 'text') return MAX_PREVIEW_TEXT_BYTES
  return 0
}

export function itemPreviewClass(item: Pick<PreviewItem, 'mime' | 'name' | 'previewClass'>): PreviewClass {
  return item.previewClass ?? classifyPreview(item.mime, item.name)
}

export function isItemPreviewable(
  item: Pick<PreviewItem, 'mime' | 'name' | 'size' | 'status' | 'previewClass' | 'previewable'>
): boolean {
  if (item.status === 'cloud_only') return false
  if (typeof item.previewable === 'boolean') return item.previewable
  const kind = itemPreviewClass(item)
  const limit = previewSizeLimit(kind)
  return limit > 0 && item.size <= limit
}

export function matchesPreviewFilter(item: PreviewItem, filter: PreviewFilter): boolean {
  if (filter === 'all') return true
  if (filter === 'previewable') return isItemPreviewable(item)
  const kind = itemPreviewClass(item)
  if (filter === 'images') return kind === 'image'
  return kind === 'markdown' || kind === 'text'
}

export function filterCounts(items: readonly PreviewItem[]): Record<PreviewFilter, number> {
  let previewable = 0
  let images = 0
  let documents = 0
  for (const item of items) {
    if (isItemPreviewable(item)) previewable++
    const kind = itemPreviewClass(item)
    if (kind === 'image') images++
    else if (kind === 'markdown' || kind === 'text') documents++
  }
  return { all: items.length, previewable, images, documents }
}

/**
 * Apply `nextFilter` while preserving the active item when it still matches,
 * or jumping to the nearest matching neighbor in the full list when it does not.
 */
export function pickNearestFiltered(
  items: readonly PreviewItem[],
  currentKey: string | null,
  nextFilter: PreviewFilter
): { filtered: PreviewItem[]; activeKey: string | null } {
  const filtered = items.filter((item) => matchesPreviewFilter(item, nextFilter))
  if (!filtered.length) return { filtered, activeKey: null }
  if (currentKey && filtered.some((item) => item.key === currentKey)) {
    return { filtered, activeKey: currentKey }
  }
  const origin = currentKey ? items.findIndex((item) => item.key === currentKey) : -1
  if (origin < 0) return { filtered, activeKey: filtered[0].key }
  const allowed = new Set(filtered.map((item) => item.key))
  for (let delta = 1; delta < items.length; delta++) {
    const forward = items[origin + delta]
    if (forward && allowed.has(forward.key)) return { filtered, activeKey: forward.key }
    const backward = items[origin - delta]
    if (backward && allowed.has(backward.key)) return { filtered, activeKey: backward.key }
  }
  return { filtered, activeKey: filtered[0].key }
}

export function clampZoom(scale: number): number {
  return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, scale))
}

export function stepZoom(current: number, direction: 1 | -1): number {
  if (direction > 0) {
    const next = ZOOM_STEPS.find((step) => step > current + 0.01)
    return next ?? ZOOM_MAX
  }
  const prev = [...ZOOM_STEPS].reverse().find((step) => step < current - 0.01)
  return prev ?? Math.min(current, ZOOM_MIN)
}
