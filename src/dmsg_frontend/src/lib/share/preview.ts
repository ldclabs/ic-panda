import markdownit from 'markdown-it'
import type { ShareFile } from './decrypt'

const TEXT_LIMIT = 2 * 1024 * 1024
const IMAGE_LIMIT = 20 * 1024 * 1024
const images = new Set([
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
  'image/avif'
])
const textTypes = new Set([
  'application/json',
  'application/xml',
  'application/yaml',
  'application/toml',
  'application/javascript',
  'image/svg+xml'
])
const textExtensions = new Set([
  'txt',
  'log',
  'md',
  'markdown',
  'csv',
  'json',
  'yaml',
  'yml',
  'toml',
  'xml',
  'html',
  'htm',
  'css',
  'js',
  'mjs',
  'ts',
  'rs',
  'py',
  'sh',
  'sql'
])

export function fileType(file: Pick<ShareFile, 'name' | 'mime' | 'size'>) {
  const extension =
    /\.([a-z0-9]{1,16})$/i.exec(file.name)?.[1]?.toLowerCase() ?? ''
  const mime = file.mime.split(';')[0]!.trim().toLowerCase()
  const text =
    mime.startsWith('text/') ||
    textTypes.has(mime) ||
    ((!mime || mime === 'application/octet-stream') &&
      textExtensions.has(extension))
  const kind = images.has(mime)
    ? 'image'
    : text
      ? mime === 'text/markdown' || ['md', 'markdown'].includes(extension)
        ? 'markdown'
        : 'text'
      : null
  return {
    mime: mime || 'application/octet-stream',
    extension,
    kind,
    previewable:
      kind !== null &&
      file.size <= (kind === 'image' ? IMAGE_LIMIT : TEXT_LIMIT)
  }
}

// A dedicated renderer: no raw HTML or automatic requests for embedded images.
const md = markdownit({ html: false, linkify: true, breaks: true })
md.renderer.rules['image'] = (tokens, index) =>
  md.utils.escapeHtml(tokens[index]!.content)
md.renderer.rules['link_open'] = (tokens, index, options, env, renderer) => {
  tokens[index]!.attrSet('target', '_blank')
  tokens[index]!.attrSet('rel', 'noopener noreferrer')
  tokens[index]!.attrSet('referrerpolicy', 'no-referrer')
  return renderer.renderToken(tokens, index, options)
}

export function renderMarkdown(text: string): string {
  return md.render(text)
}

export function readText(bytes: Uint8Array): string {
  const encoding =
    bytes[0] === 0xff && bytes[1] === 0xfe
      ? 'utf-16le'
      : bytes[0] === 0xfe && bytes[1] === 0xff
        ? 'utf-16be'
        : 'utf-8'
  const text = new TextDecoder(encoding, { fatal: true }).decode(bytes)
  if (text.includes('\0')) throw new Error('Not a text file')
  return text
}
