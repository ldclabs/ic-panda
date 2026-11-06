/** Web and dMsg addresses in plain text, for messages and letters. Nothing is fetched or previewed. */
export type LinkPart = { text: string; url?: undefined } | { text: string; url: string; host: string }

const ADDRESS = /(?:https?|dmsg):\/\/[^\s<>"'`，。！？；：（）【】《》「」『』、]+/gi

const unbalanced = (text: string, open: string, close: string) => text.split(close).length > text.split(open).length

/** Trailing punctuation belongs to the sentence, a closing bracket only when the address did not open it. */
function trimmed(address: string): string {
  let end = address.length
  while (end > 0) {
    const last = address[end - 1]
    if ('.,;:!?*_~'.includes(last)) end--
    else if (last === ')' && unbalanced(address.slice(0, end), '(', ')')) end--
    else if (last === ']' && unbalanced(address.slice(0, end), '[', ']')) end--
    else break
  }
  return address.slice(0, end)
}

/** The text split around its addresses; only what parses as a URL becomes a link. */
export function linkParts(text: string): LinkPart[] {
  const parts: LinkPart[] = []
  let at = 0
  for (const match of text.matchAll(ADDRESS)) {
    const address = trimmed(match[0])
    let url: URL
    try {
      url = new URL(address)
    } catch {
      continue
    }
    if (match.index > at) parts.push({ text: text.slice(at, match.index) })
    parts.push({ text: address, url: address, host: url.hostname || url.protocol })
    at = match.index + address.length
  }
  if (at < text.length) parts.push({ text: text.slice(at) })
  return parts
}
