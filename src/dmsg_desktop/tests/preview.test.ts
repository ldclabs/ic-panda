import assert from 'node:assert/strict'
import { test } from 'node:test'
import { WINDOW_METHODS } from '../src/main/policy.ts'
import { isRead } from '../src/shared/requests.ts'
import {
  clampZoom,
  classifyPreview,
  filterCounts,
  isItemPreviewable,
  MAX_PREVIEW_IMAGE_BYTES,
  MAX_PREVIEW_MARKDOWN_BYTES,
  MAX_PREVIEW_TEXT_BYTES,
  matchesPreviewFilter,
  pickNearestFiltered,
  stepZoom,
  type PreviewItem
} from '../src/renderer/components/preview.ts'

const item = (key: string, name: string, mime: string, size: number, status?: string): PreviewItem => ({
  key,
  source: { source: 'library', id: key },
  name,
  mime,
  size,
  status
})

test('channel/attachments is exposed to windows as a read-only query', () => {
  assert.equal(WINDOW_METHODS.has('channel/attachments'), true)
  assert.equal(isRead('channel/attachments'), true)
})

test('preview classification matches supported raster images, markdown, text, and excludes svg/secrets', () => {
  assert.equal(classifyPreview('image/png', 'photo.png'), 'image')
  assert.equal(classifyPreview('application/octet-stream', 'photo.WEBP'), 'image')
  assert.equal(classifyPreview('image/svg+xml', 'icon.svg'), 'unsupported')
  assert.equal(classifyPreview('text/plain', 'README.md'), 'markdown')
  assert.equal(classifyPreview('text/markdown', 'notes'), 'markdown')
  assert.equal(classifyPreview('application/json', 'config.json'), 'text')
  assert.equal(classifyPreview('application/octet-stream', 'main.rs'), 'text')
  assert.equal(classifyPreview('application/pdf', 'report.pdf'), 'unsupported')
  assert.equal(classifyPreview('application/vnd.dmsg.secret-package', 'secrets.md'), 'unsupported')
})

test('previewable check enforces size budgets and cloud-only status', () => {
  assert.equal(isItemPreviewable(item('a', 'pic.png', 'image/png', MAX_PREVIEW_IMAGE_BYTES)), true)
  assert.equal(isItemPreviewable(item('b', 'big.png', 'image/png', MAX_PREVIEW_IMAGE_BYTES + 1)), false)
  assert.equal(isItemPreviewable(item('c', 'doc.md', 'text/markdown', MAX_PREVIEW_MARKDOWN_BYTES)), true)
  assert.equal(isItemPreviewable(item('d', 'huge.md', 'text/markdown', MAX_PREVIEW_MARKDOWN_BYTES + 1)), false)
  assert.equal(isItemPreviewable(item('e', 'log.txt', 'text/plain', MAX_PREVIEW_TEXT_BYTES)), true)
  assert.equal(isItemPreviewable(item('f', 'cloud.png', 'image/png', 1024, 'cloud_only')), false)
  assert.equal(isItemPreviewable(item('g', 'archive.zip', 'application/zip', 100)), false)
})

test('filter counts and nearest-neighbor focus preservation behave deterministically', () => {
  const list = [
    item('1', 'a.zip', 'application/zip', 100),
    item('2', 'b.png', 'image/png', 200),
    item('3', 'c.pdf', 'application/pdf', 300),
    item('4', 'd.md', 'text/markdown', 400),
    item('5', 'e.txt', 'text/plain', 500),
    item('6', 'f.png', 'image/png', MAX_PREVIEW_IMAGE_BYTES + 1)
  ]
  assert.deepEqual(filterCounts(list), { all: 6, previewable: 3, images: 2, documents: 2 })
  assert.equal(matchesPreviewFilter(list[5], 'images'), true)
  assert.equal(matchesPreviewFilter(list[5], 'previewable'), false)

  // Staying on a matching item keeps focus.
  assert.deepEqual(
    pickNearestFiltered(list, '4', 'documents').activeKey,
    '4'
  )
  // Switching from non-matching index 2 (c.pdf) to images: distance 1 backward (2: b.png) vs distance 3 forward (6: f.png) -> b.png.
  assert.equal(pickNearestFiltered(list, '3', 'images').activeKey, '2')
  // Switching from index 2 (c.pdf) to previewable: distance 1 forward (4: d.md) ties distance 1 backward (2: b.png) -> prefers forward (4).
  assert.equal(pickNearestFiltered(list, '3', 'previewable').activeKey, '4')
  // Empty filter result returns null activeKey.
  assert.deepEqual(pickNearestFiltered([list[0], list[2]], '1', 'images'), { filtered: [], activeKey: null })
})

test('zoom steps clamp within bounds and advance cleanly', () => {
  assert.equal(clampZoom(0.1), 0.25)
  assert.equal(clampZoom(10), 4.0)
  assert.equal(stepZoom(1, 1), 1.25)
  assert.equal(stepZoom(1, -1), 0.85)
  assert.equal(stepZoom(4, 1), 4)
  assert.equal(stepZoom(0.25, -1), 0.25)
})
