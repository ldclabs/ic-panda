import { untrack } from 'svelte'
import type { HighlightSpan } from '../../components/highlight'
import { call } from '../../rpc'

export interface FileHit {
  id: string
  name: string
  size: number
  status: string
  path?: string[]
  mime?: string
  starred?: boolean
  versions?: number
  updated_at?: number
  trashed_at?: number
  folder?: string
  _nameSpans?: HighlightSpan[]
  _pathSpans?: HighlightSpan[]
  _matchField?: string | null
  _matchValue?: string | null
  _score?: number
}

export interface FolderHit {
  id: string
  name: string
  parent: string
  path?: string[]
  updated_at?: number
  trashed_at?: number
  _nameSpans?: HighlightSpan[]
  _pathSpans?: HighlightSpan[]
  _score?: number
}

export interface SearchScopeOptions {
  folder?: string
  starred?: boolean
  tag?: string | null
  trash?: boolean
}

type SearchResult =
  | { kind: 'idle' | 'loading' }
  | { kind: 'ready'; query: string; scopeKey: string; folder?: string; folders: FolderHit[]; files: FileHit[] }
  | { kind: 'failed'; cause: unknown }

interface ListingResponse {
  folders: (FolderHit & { name_spans?: HighlightSpan[]; path_spans?: HighlightSpan[]; score?: number })[]
  files: (FileHit & { name_spans?: HighlightSpan[]; path_spans?: HighlightSpan[]; match_field?: string | null; match_value?: string | null; score?: number })[]
}

/** A null query closes the search. Only the current effect can publish its reply. */
export function searchLibrary(
  query: () => string | null,
  delay = 150,
  folder?: () => string | undefined,
  options?: () => SearchScopeOptions
) {
  let result = $state.raw<SearchResult>({ kind: 'idle' })
  let retry = $state(0)

  $effect(() => {
    const text = query()
    const opts = options?.() ?? {}
    const scope = opts.folder ?? folder?.()
    const starred = !!opts.starred
    const tag = opts.tag ?? null
    const trash = !!opts.trash
    const scopeKey = `${scope ?? ''}|${starred}|${tag ?? ''}|${trash}`
    void retry
    if (text === null) { result = { kind: 'idle' }; return }
    let current = true
    // A library refresh can repeat the same search while its detail view is open.
    const previous = untrack(() => result)
    if (previous.kind !== 'ready' || previous.query !== text || previous.scopeKey !== scopeKey) result = { kind: 'loading' }
    const timer = setTimeout(async () => {
      try {
        const params: Record<string, unknown> = { query: text }
        if (scope && !trash) params.folder = scope
        if (starred) params.starred = true
        if (tag) params.tag = tag
        if (trash) params.trash = true
        const reply = await call<ListingResponse>('files/search_listing', params)
        if (!current) return
        const folders: FolderHit[] = (reply.folders ?? []).map((hit) => ({
          ...hit,
          _nameSpans: hit.name_spans,
          _pathSpans: hit.path_spans,
          _score: hit.score
        }))
        const files: FileHit[] = (reply.files ?? []).map((hit) => ({
          ...hit,
          _nameSpans: hit.name_spans,
          _pathSpans: hit.path_spans,
          _matchField: hit.match_field,
          _matchValue: hit.match_value,
          _score: hit.score
        }))
        result = { kind: 'ready', query: text, scopeKey, folder: scope, folders, files }
      } catch (cause) {
        if (current) result = { kind: 'failed', cause }
      }
    }, delay)
    return () => { current = false; clearTimeout(timer) }
  })

  return { get result() { return result }, retry: () => { retry++ } }
}
