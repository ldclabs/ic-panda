export interface HighlightSpan {
  text: string
  matched: boolean
}

/** Prefer the attachment when it explains the hit and the message body does not. */
export function messagePreview(hit: {
  message: { text: string; file?: { name: string } | null; file_deleted?: boolean }
  snippet_spans?: HighlightSpan[]
  file_spans?: HighlightSpan[]
}) {
  const { message, snippet_spans, file_spans } = hit
  if (message.file && !message.file_deleted &&
      (!message.text || (file_spans?.some(s => s.matched) && !snippet_spans?.some(s => s.matched)))) {
    return { text: message.file.name, spans: file_spans }
  }
  return { text: message.text, spans: snippet_spans }
}
