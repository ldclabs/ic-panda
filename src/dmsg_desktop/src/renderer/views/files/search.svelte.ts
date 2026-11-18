import { untrack } from 'svelte'
import { call } from '../../rpc'

interface FileHit {
  id: string
  name: string
  size: number
  status: string
}

type SearchResult =
  | { kind: 'idle' | 'loading' }
  | { kind: 'ready'; query: string; folder?: string; files: FileHit[] }
  | { kind: 'failed'; cause: unknown }

/** A null query closes the search. Only the current effect can publish its reply. */
export function searchLibrary(query: () => string | null, delay = 150, folder?: () => string) {
  let result = $state.raw<SearchResult>({ kind: 'idle' })
  let retry = $state(0)

  $effect(() => {
    const text = query()
    const scope = folder?.()
    void retry
    if (text === null) { result = { kind: 'idle' }; return }
    let current = true
    // A library refresh can repeat the same search while its detail view is open.
    const previous = untrack(() => result)
    if (previous.kind !== 'ready' || previous.query !== text || previous.folder !== scope) result = { kind: 'loading' }
    const timer = setTimeout(async () => {
      try {
        const files = await call<FileHit[]>('files/search', scope ? { query: text, folder: scope } : { query: text })
        if (current) result = { kind: 'ready', query: text, folder: scope, files }
      } catch (cause) {
        if (current) result = { kind: 'failed', cause }
      }
    }, delay)
    return () => { current = false; clearTimeout(timer) }
  })

  return { get result() { return result }, retry: () => { retry++ } }
}
