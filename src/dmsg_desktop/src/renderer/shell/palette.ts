/**
 * What the command palette finds. It matches and caps its own rows: a vault
 * of thousands would otherwise put every row in the page on the first key.
 */
export const GROUP_LIMIT = 20

export const tokensOf = (search: string) => search.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean)

/** 3 when the label starts with the search, 2 when it holds every word, 1 when its keywords help; 0 is no match. */
export function matchScore(label: string, keywords: readonly string[] | undefined, tokens: readonly string[]): number {
  const name = label.toLocaleLowerCase()
  const rest = (keywords ?? []).join(' ').toLocaleLowerCase()
  if (!tokens.every((token) => name.includes(token) || rest.includes(token))) return 0
  if (tokens.length && name.startsWith(tokens[0])) return 3
  return tokens.every((token) => name.includes(token)) ? 2 : 1
}

/** The entries that match, best first and otherwise in their own order, at most `limit`. */
export function best<T extends { label: string; keywords?: string[] }>(entries: readonly T[], tokens: readonly string[], limit = GROUP_LIMIT): T[] {
  const scored: { entry: T; score: number }[] = []
  for (const entry of entries) {
    const score = matchScore(entry.label, entry.keywords, tokens)
    if (score) scored.push({ entry, score })
  }
  return scored.sort((a, b) => b.score - a.score).slice(0, limit).map(({ entry }) => entry)
}
