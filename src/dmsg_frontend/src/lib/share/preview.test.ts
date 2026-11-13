import { expect, it } from 'vitest'
import { fileType, readText, renderMarkdown } from './preview'

it('offers bounded reading and raster previews, leaving active documents as text', () => {
  const kind = (name: string, mime: string, size = 100) =>
    fileType({ name, mime, size })
  expect(kind('notes.MD', 'application/octet-stream')).toMatchObject({
    kind: 'markdown',
    previewable: true
  })
  expect(kind('image.png', 'image/png')).toMatchObject({
    kind: 'image',
    previewable: true
  })
  expect(kind('page.html', 'text/html').kind).toBe('text')
  expect(kind('drawing.svg', 'image/svg+xml').kind).toBe('text')
  expect(kind('program.txt.exe', 'application/octet-stream').kind).toBeNull()
  expect(kind('archive.md', 'application/zip').kind).toBeNull()
  expect(kind('notes.txt', 'text/plain', 3 * 1024 * 1024).previewable).toBe(
    false
  )
  expect(kind('photo.jpg', 'image/jpeg', 21 * 1024 * 1024).previewable).toBe(
    false
  )
  expect(kind('empty.txt', 'text/plain', 0).previewable).toBe(true)
})

it('renders Markdown without active HTML, embedded requests or script links', () => {
  const html = renderMarkdown(
    '# Title\n\n<script>alert(1)</script>\n\n<img src="https://tracker.test/pixel">\n\n![tracking](https://tracker.test/image)\n\n[bad](javascript:alert(1))\n\n[link](https://example.com)'
  )
  const document = new DOMParser().parseFromString(html, 'text/html')
  expect(document.querySelector('h1')?.textContent).toBe('Title')
  expect(
    document.querySelector('script, img, iframe, object, style')
  ).toBeNull()
  expect(document.querySelector('a[href="https://example.com"]')).not.toBeNull()
  for (const link of document.querySelectorAll('a')) {
    expect(link.href.startsWith('https://')).toBe(true)
    expect(link.getAttribute('target')).toBe('_blank')
    expect(link.getAttribute('rel')).toBe('noopener noreferrer')
    expect(link.getAttribute('referrerpolicy')).toBe('no-referrer')
  }
})

it('reads UTF-8 and BOM-marked UTF-16, rejecting binary or invalid text', () => {
  expect(readText(new TextEncoder().encode('你好\nمرحبا'))).toBe('你好\nمرحبا')
  expect(readText(new Uint8Array([0xff, 0xfe, 65, 0]))).toBe('A')
  expect(readText(new Uint8Array([0xfe, 0xff, 0, 65]))).toBe('A')
  expect(readText(new Uint8Array())).toBe('')
  expect(() => readText(new Uint8Array([0xff]))).toThrow()
  expect(() => readText(new Uint8Array([0, 1, 2]))).toThrow()
})
