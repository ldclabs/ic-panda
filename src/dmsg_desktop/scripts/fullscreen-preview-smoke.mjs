// Real Electron renderer with synthetic daemon + HTTP preview server; verifies
// fullscreen file preview across Files and Channels (images, Markdown, text,
// unsupported fallback, filters, keyboard navigation, narrow windows, RTL, themes).
import { _electron as electron } from 'playwright'
import electronPath from 'electron'
import { createServer } from 'node:http'
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import assert from 'node:assert/strict'
import { WebSocketServer } from 'ws'

const account = '040g2081040g2081040g'
const peer = '0123456789abcdefghi0'
const channelId = '33'.repeat(32)
const token = 'preview-ui-fixture'
const now = Date.now()
const calls = []
const previewRequests = []
const sockets = new Set()

const pngBytes = await readFile(resolve('resources/icon.png'))
const pngDataUrl = `data:image/png;base64,${pngBytes.toString('base64')}`

const sampleMarkdownRaw = `# dMsg 全屏预览架构说明

支持 **Markdown 阅读视图**、*斜体*、~~删除线~~ 与行内代码 \`dmsg preview\`。

> 安全策略：原始 HTML 已剥离，外链图片默认拦截不自动发起请求。

- [x] 图片灯箱缩放与拖拽
- [x] 顶部筛选器（全部 / 可预览 / 图片 / 文档）
- [ ] 更多格式后续按需扩展

| 类型 | 上限 | 渲染方式 |
| :--- | :---: | ---: |
| Image | 20 MiB | Object URL |
| Markdown | 2 MiB | Safe AST |

\`\`\`rust
pub fn preview_ready(bytes: usize) -> bool {
    bytes <= 2 * 1024 * 1024
}
\`\`\`

![External preview diagram](https://example.invalid/diagram.png)

更多说明见 [dMsg 文档](https://dmsg.net) 。
`

const sampleMarkdownDoc = {
  truncated: false,
  blocks: [
    { type: 'heading', level: 1, inlines: [{ type: 'text', text: 'dMsg 全屏预览架构说明' }] },
    {
      type: 'paragraph',
      inlines: [
        { type: 'text', text: '支持 ' },
        { type: 'strong', children: [{ type: 'text', text: 'Markdown 阅读视图' }] },
        { type: 'text', text: '、' },
        { type: 'emphasis', children: [{ type: 'text', text: '斜体' }] },
        { type: 'text', text: '、' },
        { type: 'strikethrough', children: [{ type: 'text', text: '删除线' }] },
        { type: 'text', text: ' 与行内代码 ' },
        { type: 'code', code: 'dmsg preview' },
        { type: 'text', text: '。' }
      ]
    },
    {
      type: 'block_quote',
      blocks: [
        { type: 'paragraph', inlines: [{ type: 'text', text: '安全策略：原始 HTML 已剥离，外链图片默认拦截不自动发起请求。' }] }
      ]
    },
    {
      type: 'list',
      ordered: false,
      start: 1,
      items: [
        { checked: true, blocks: [{ type: 'paragraph', inlines: [{ type: 'text', text: '图片灯箱缩放与拖拽' }] }] },
        { checked: true, blocks: [{ type: 'paragraph', inlines: [{ type: 'text', text: '顶部筛选器（全部 / 可预览 / 图片 / 文档）' }] }] },
        { checked: false, blocks: [{ type: 'paragraph', inlines: [{ type: 'text', text: '更多格式后续按需扩展' }] }] }
      ]
    },
    {
      type: 'table',
      alignments: ['left', 'center', 'right'],
      headers: [
        [{ type: 'text', text: '类型' }],
        [{ type: 'text', text: '上限' }],
        [{ type: 'text', text: '渲染方式' }]
      ],
      rows: [
        [[{ type: 'text', text: 'Image' }], [{ type: 'text', text: '20 MiB' }], [{ type: 'text', text: 'Object URL' }]],
        [[{ type: 'text', text: 'Markdown' }], [{ type: 'text', text: '2 MiB' }], [{ type: 'text', text: 'Safe AST' }]]
      ]
    },
    {
      type: 'code_block',
      language: 'rust',
      code: 'pub fn preview_ready(bytes: usize) -> bool {\n    bytes <= 2 * 1024 * 1024\n}'
    },
    {
      type: 'paragraph',
      inlines: [{ type: 'blocked_image', alt: 'External preview diagram' }]
    },
    {
      type: 'paragraph',
      inlines: [
        { type: 'text', text: '更多说明见 ' },
        { type: 'link', url: 'https://dmsg.net', children: [{ type: 'text', text: 'dMsg 文档' }] },
        { type: 'text', text: ' 。' }
      ]
    }
  ]
}

const sampleJsonText = JSON.stringify({ service: 'dmsg', preview: true, maxImageMiB: 20, formats: ['image', 'markdown', 'text'] }, null, 2)

const libraryFiles = [
  {
    id: '11'.repeat(32),
    name: '01-design-mockup.png',
    folder: 'root',
    path: [],
    mime: 'image/png',
    size: pngBytes.length,
    current: 'v1',
    sha256: 'a1'.repeat(32),
    status: 'local',
    local: true,
    starred: true,
    tags: ['design'],
    note: '',
    created_at: now - 300_000,
    updated_at: now - 300_000,
    versions: 1
  },
  {
    id: '22'.repeat(32),
    name: '02-architecture-notes.md',
    folder: 'root',
    path: [],
    mime: 'text/markdown',
    size: Buffer.byteLength(sampleMarkdownRaw),
    current: 'v1',
    sha256: 'b2'.repeat(32),
    status: 'local',
    local: true,
    starred: false,
    tags: ['docs'],
    note: '',
    created_at: now - 200_000,
    updated_at: now - 200_000,
    versions: 1
  },
  {
    id: '33'.repeat(32),
    name: '03-config.json',
    folder: 'root',
    path: [],
    mime: 'application/json',
    size: Buffer.byteLength(sampleJsonText),
    current: 'v1',
    sha256: 'c3'.repeat(32),
    status: 'local',
    local: true,
    starred: false,
    tags: [],
    note: '',
    created_at: now - 100_000,
    updated_at: now - 100_000,
    versions: 1
  },
  {
    id: '44'.repeat(32),
    name: '04-quarterly-report.pdf',
    folder: 'root',
    path: [],
    mime: 'application/pdf',
    size: 1_842_000,
    current: 'v1',
    sha256: 'd4'.repeat(32),
    status: 'local',
    local: true,
    starred: false,
    tags: [],
    note: '',
    created_at: now - 50_000,
    updated_at: now - 50_000,
    versions: 1
  }
]

const channel = {
  id: channelId,
  name: '设计评审 · Design Review',
  genesis: '44'.repeat(32),
  ledger: {
    channel_id: channelId,
    type: 'collaboration',
    owner: account,
    epoch: 1,
    status: 'active',
    members: {
      [account]: { account, role: 'owner', joined_epoch: 1, can_rotate: true, history: [] },
      [peer]: { account: peer, role: 'member', joined_epoch: 1, can_rotate: false, history: [] }
    }
  },
  message_cursor: 3,
  readable_epochs: [1]
}

const channelMessages = [
  {
    seq: 1,
    account: peer,
    text: '最新视觉稿如下：',
    message_id: '0'.repeat(63) + '1',
    digest: 'e1'.repeat(32),
    epoch: 1,
    created_at: now - 180_000,
    stored_at: now - 180_000,
    file: { name: 'channel-mockup.png', mime: 'image/png', size: pngBytes.length }
  },
  {
    seq: 2,
    account,
    text: '附上设计说明文档：',
    message_id: '0'.repeat(63) + '2',
    digest: 'e2'.repeat(32),
    epoch: 1,
    created_at: now - 120_000,
    stored_at: now - 120_000,
    file: { name: 'review-notes.md', mime: 'text/markdown', size: Buffer.byteLength(sampleMarkdownRaw) }
  },
  {
    seq: 3,
    account: peer,
    text: '归档 PDF：',
    message_id: '0'.repeat(63) + '3',
    digest: 'e3'.repeat(32),
    epoch: 1,
    created_at: now - 60_000,
    stored_at: now - 60_000,
    file: { name: 'archive-bundle.pdf', mime: 'application/pdf', size: 2_400_000 }
  }
]

const channelAttachments = [
  {
    seq: 1, digest: 'e1'.repeat(32), account: peer, created_at: now - 180_000, stored_at: now - 180_000,
    name: 'channel-mockup.png', mime: 'image/png', size: pngBytes.length, preview_class: 'image', previewable: true
  },
  {
    seq: 2, digest: 'e2'.repeat(32), account, created_at: now - 120_000, stored_at: now - 120_000,
    name: 'review-notes.md', mime: 'text/markdown', size: Buffer.byteLength(sampleMarkdownRaw), preview_class: 'markdown', previewable: true
  },
  {
    seq: 3, digest: 'e3'.repeat(32), account: peer, created_at: now - 60_000, stored_at: now - 60_000,
    name: 'archive-bundle.pdf', mime: 'application/pdf', size: 2_400_000, preview_class: 'unsupported', previewable: false
  }
]

const status = () => ({
  state: 'unlocked',
  session: 'fixture',
  account,
  device_id: '55'.repeat(32),
  approvals_waiting: 0,
  pending_sync: 0,
  background_online: false,
  fast_unlock: false,
  idle_lock_ms: 900000
})

const methods = {
  'app/initialize': () => ({ protocol: 'dmsg-app/1', status: status() }),
  'app/activity': () => null,
  'session/status': status,
  'daemon/status': status,
  'session/accounts': () => [],
  'profile/display': () => ({ name: 'Me', avatar: null }),
  'profile/show': () => null,
  'approval/list': () => [],
  'legacy/channels': () => ({ channels: [], errors: [] }),
  'legacy/list': () => [],
  'legacy/pairs': () => [],
  'files/tree': () => [],
  'files/tags': () => [{ tag: 'design', files: 1 }, { tag: 'docs', files: 1 }],
  'files/share_links': () => [],
  'files/backup_status': () => [],
  'files/download_status': () => [],
  'attest/history': () => [],
  'files/usage': () => ({ files: libraryFiles.length, trashed: 0, versions: libraryFiles.length, local_versions: libraryFiles.length, local_bytes: 1_900_000, bytes: 1_900_000, open_copies: 0, trash_days: 30 }),
  'files/list': () => ({ folders: [], files: libraryFiles }),
  'files/show': (p) => {
    const file = libraryFiles.find((f) => f.id === p.id) ?? libraryFiles[0]
    return {
      file,
      versions: [{ id: file.current, size: file.size, sha256: file.sha256, mime: file.mime, created_at: file.created_at, device: '55'.repeat(32), local: true, current: true, status: 'local', backup: null }]
    }
  },
  'files/preview': (p) => {
    if (p.id === libraryFiles[0].id) return { kind: 'image', mime: 'image/png', url: pngDataUrl }
    if (p.id === libraryFiles[1].id) return { kind: 'text', mime: 'text/markdown', text: sampleMarkdownRaw }
    return { kind: 'none', mime: 'application/octet-stream' }
  },
  'files/mount_status': () => ({ mounted: false, supported: false }),
  'channel/list': () => [channel],
  'channel/show': () => channel,
  'channel/message_page': () => ({ items: channelMessages, has_more: false, next_before: null }),
  'channel/messages': () => [],
  'channel/attachments': () => channelAttachments,
  'channel/preview': (p) => (p.seq === 1 ? { url: pngDataUrl } : null),
  'channel/sync': () => ({ through: 3, received: 0 }),
  'channel/outbox': () => [],
  'channel/receipts': () => ({ latest_seq: 3, cursors: {} }),
  'channel/mark': () => channel,
  'channel/draft': () => null,
  'channel/names': (p) => Object.fromEntries((p.accounts ?? []).map((a) => [a, { name: a === account ? 'Me' : 'ICPanda', avatar_url: null, stale: false }])),
  'notifications/context': () => null,
  'notifications/observed': () => null,
  'notifications/next': () => ({ plans: [], remove: [] }),
  'notifications/status': () => ({ counts: { action: 0, message: 0, security: 0, service: 0 }, badge: 0 })
}

function sendJsonPreview(res, payload) {
  const body = Buffer.from(JSON.stringify(payload), 'utf8')
  res.writeHead(200, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
    'x-dmsg-preview-kind': 'json'
  })
  res.end(body)
}

function sendImagePreview(res, meta) {
  res.writeHead(200, {
    'content-type': 'image/png',
    'cache-control': 'no-store',
    'x-dmsg-preview-kind': 'image',
    'x-dmsg-preview-meta': Buffer.from(JSON.stringify({
      kind: 'image', mime: 'image/png', name: meta.name, size: pngBytes.length, sha256: meta.sha256
    })).toString('base64url')
  })
  res.end(pngBytes)
}

const server = createServer((req, res) => {
  if (req.method === 'POST' && req.url === '/app/v1/preview') {
    if (req.headers.authorization !== `Bearer ${token}`) {
      res.writeHead(401).end()
      return
    }
    const chunks = []
    req.on('data', (c) => chunks.push(c))
    req.on('end', () => {
      const body = JSON.parse(Buffer.concat(chunks).toString('utf8'))
      previewRequests.push(body)
      if (body.source === 'library') {
        const file = libraryFiles.find((f) => f.id === body.id)
        if (!file) { res.writeHead(404).end(); return }
        if (file.mime === 'image/png') return sendImagePreview(res, file)
        if (file.mime === 'text/markdown') {
          return sendJsonPreview(res, {
            kind: 'markdown', mime: file.mime, name: file.name, size: file.size, sha256: file.sha256,
            document: sampleMarkdownDoc, text: sampleMarkdownRaw
          })
        }
        if (file.mime === 'application/json') {
          return sendJsonPreview(res, {
            kind: 'text', mime: file.mime, name: file.name, size: file.size, sha256: file.sha256,
            text: sampleJsonText
          })
        }
        return sendJsonPreview(res, {
          kind: 'unsupported', mime: file.mime, name: file.name, size: file.size, sha256: file.sha256,
          reason: 'unsupported_type'
        })
      }
      if (body.source === 'channel') {
        const att = channelAttachments.find((a) => a.seq === body.seq)
        if (!att) { res.writeHead(404).end(); return }
        if (att.mime === 'image/png') return sendImagePreview(res, att)
        if (att.mime === 'text/markdown') {
          return sendJsonPreview(res, {
            kind: 'markdown', mime: att.mime, name: att.name, size: att.size,
            document: sampleMarkdownDoc, text: sampleMarkdownRaw
          })
        }
        return sendJsonPreview(res, {
          kind: 'unsupported', mime: att.mime, name: att.name, size: att.size,
          reason: 'unsupported_type'
        })
      }
      res.writeHead(400).end()
    })
    return
  }
  res.writeHead(404).end()
})

const wss = new WebSocketServer({ noServer: true })
server.on('upgrade', (req, socket, head) => {
  if (req.headers.authorization !== `Bearer ${token}`) return socket.destroy()
  wss.handleUpgrade(req, socket, head, (ws) => {
    sockets.add(ws)
    ws.on('close', () => sockets.delete(ws))
    ws.on('message', async (bytes) => {
      const m = JSON.parse(String(bytes))
      calls.push(m)
      try {
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, result: (await methods[m.method]?.(m.params ?? {})) ?? null }))
      } catch {
        ws.send(JSON.stringify({ jsonrpc: '2.0', id: m.id, error: { code: -32000, message: 'Unavailable', data: { code: 'UNAVAILABLE' } } }))
      }
    })
  })
})

await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
const profile = await mkdtemp(join(tmpdir(), 'dmsg-preview-ui-'))
const shots = resolve(process.env.DMSG_PREVIEW_SHOTS || 'test-results/fullscreen-preview')
await mkdir(shots, { recursive: true })
await writeFile(join(profile, 'locale'), 'en')
await writeFile(join(profile, 'appearance'), 'light')

const app = await electron.launch({
  executablePath: electronPath,
  colorScheme: null,
  args: [resolve('.')],
  env: {
    ...process.env,
    DMSG_DESKTOP_TEST: '1',
    DMSG_DESKTOP_USER_DATA: profile,
    DMSG_DESKTOP_DAEMON: `http://127.0.0.1:${server.address().port}`,
    DMSG_DESKTOP_TOKEN: token,
    DMSG_HOME: join(profile, 'home')
  }
})

try {
  const main = await app.firstWindow()
  main.setDefaultTimeout(15000)
  const failures = []
  main.on('pageerror', (error) => failures.push(`${main.url()} ${error.stack}`))
  const win = await app.browserWindow(main)

  const setLook = async (loc, theme, width, height) => {
    await win.evaluate((w, [cw, ch]) => w.setContentSize(cw, ch), [width, height])
    await main.evaluate(async ({ loc, theme }) => {
      await window.dmsg.setLocale(loc)
      await window.dmsg.setAppearance(theme)
    }, { loc, theme })
    await main.waitForSelector(`html[lang|="${loc}"]`)
    await main.waitForFunction((dark) => matchMedia('(prefers-color-scheme: dark)').matches === dark, theme === 'dark')
    await main.waitForTimeout(120)
  }

  // 1. Files workspace fullscreen preview
  await setLook('en', 'light', 1180, 760)
  await main.evaluate(() => { location.hash = '/files' })
  await main.getByTestId('files-list').waitFor()
  const imageRow = main.locator('[data-testid="files-list"] [data-kind="file"]').first()
  await imageRow.waitFor()
  await imageRow.dblclick()

  const lightbox = main.getByTestId('file-lightbox')
  await lightbox.waitFor()
  await main.getByTestId('lightbox-image').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), '01-design-mockup.png')
  assert.equal(await main.getByTestId('lightbox-counter').innerText(), '1 / 4')
  const titleCentered = await main.getByTestId('lightbox-filename').evaluate((title) => {
    const centerY = (node) => {
      const { top, height } = node.getBoundingClientRect()
      return top + height / 2
    }
    return Math.abs(centerY(title) - centerY(title.closest('header'))) <= 1
  })
  assert.ok(titleCentered, 'the filename is vertically centered in the preview toolbar')
  assert.equal(await lightbox.evaluate((node) => getComputedStyle(node).getPropertyValue('app-region')), 'no-drag')

  // Verify zoom controls (+, reset)
  const beforeZoom = await main.getByTestId('lightbox-zoom-reset').innerText()
  await main.getByTestId('lightbox-zoom-in').click()
  const afterZoom = await main.getByTestId('lightbox-zoom-reset').innerText()
  assert.notEqual(afterZoom, beforeZoom, 'zoom-in should update zoom percentage')
  await main.getByTestId('lightbox-zoom-reset').click()
  assert.ok(previewRequests.some((request) => request.source === 'library' && request.version === libraryFiles[0].current))
  await main.keyboard.press('0')
  await main.getByTestId('lightbox-image').dblclick()
  await main.getByTestId('lightbox-image').dblclick()
  assert.equal(await main.getByTestId('lightbox-zoom-reset').innerText(), beforeZoom, 'double-clicking a zoomed image should restore fit')

  await main.screenshot({ path: join(shots, 'desktop-files-image-en-light-1180.png'), animations: 'disabled' })
  await setLook('en', 'dark', 1180, 760)
  await main.screenshot({ path: join(shots, 'desktop-files-image-en-dark-1180.png'), animations: 'disabled' })

  // Navigate to Markdown via keyboard ArrowRight
  await main.keyboard.press('ArrowRight')
  await main.getByTestId('lightbox-markdown').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), '02-architecture-notes.md')
  assert.equal(await main.getByTestId('lightbox-counter').innerText(), '2 / 4')
  assert.ok((await main.getByTestId('lightbox-markdown').innerText()).includes('dMsg 全屏预览架构说明'))
  assert.ok((await main.getByTestId('lightbox-markdown').innerText()).includes('External preview diagram'))

  await setLook('zh', 'light', 1180, 760)
  await main.screenshot({ path: join(shots, 'desktop-files-markdown-zh-light-1180.png'), animations: 'disabled' })
  await setLook('zh', 'dark', 1180, 760)
  await main.screenshot({ path: join(shots, 'desktop-files-markdown-zh-dark-1180.png'), animations: 'disabled' })

  // Reading / Source toggle
  await main.getByTestId('lightbox-md-source').click()
  await main.getByTestId('lightbox-markdown-source').waitFor()
  assert.ok((await main.getByTestId('lightbox-markdown-source').innerText()).includes('# dMsg 全屏预览架构说明'))
  await main.getByTestId('lightbox-md-reading').click()
  await main.getByTestId('lightbox-markdown').waitFor()

  // Filter: Documents -> 1 / 2 -> Next -> JSON plain text -> All -> Next -> PDF fallback -> Previewable (nearest neighbor!)
  await main.getByTestId('lightbox-filter-documents').click()
  assert.equal(await main.getByTestId('lightbox-counter').innerText(), '1 / 2')
  await main.getByTestId('lightbox-next').click()
  await main.getByTestId('lightbox-text').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), '03-config.json')
  await main.getByTestId('lightbox-filter-all').focus()
  await main.keyboard.press('Space')
  assert.equal(await main.getByTestId('lightbox-counter').innerText(), '3 / 4')
  await main.getByTestId('lightbox-next').click()
  await main.getByTestId('lightbox-fallback').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), '04-quarterly-report.pdf')

  // Narrow window (760x540) + Chinese + fallback + dropdown filter
  await setLook('zh', 'light', 760, 540)
  await main.getByTestId('lightbox-filter-dropdown').waitFor()
  assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no horizontal overflow at 760px')
  await main.screenshot({ path: join(shots, 'desktop-files-fallback-zh-light-760.png'), animations: 'disabled' })

  // Switch filter via narrow dropdown to Previewable -> nearest neighbor jumps from PDF (index 3) to JSON (index 2)
  await main.getByTestId('lightbox-filter-dropdown').click()
  await main.getByTestId('lightbox-filter-option-previewable').click()
  await main.getByTestId('lightbox-text').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), '03-config.json')
  assert.equal(await main.getByTestId('lightbox-counter').innerText(), '3 / 3')

  // Arabic RTL + narrow window + dark image & light Markdown
  await setLook('ar', 'dark', 760, 540)
  await main.getByTestId('lightbox-filter-dropdown').click()
  await main.getByTestId('lightbox-filter-option-images').click()
  await main.getByTestId('lightbox-image').waitFor()
  assert.ok(await main.evaluate(() => document.documentElement.dir === 'rtl' && document.documentElement.scrollWidth <= innerWidth))
  await main.screenshot({ path: join(shots, 'desktop-files-image-ar-dark-760.png'), animations: 'disabled' })

  await setLook('ar', 'light', 760, 540)
  await main.getByTestId('lightbox-filter-dropdown').click()
  await main.getByTestId('lightbox-filter-option-documents').click()
  await main.getByTestId('lightbox-markdown').waitFor()
  assert.ok(await main.evaluate(() => document.documentElement.scrollWidth <= innerWidth))
  await main.screenshot({ path: join(shots, 'desktop-files-markdown-ar-light-760.png'), animations: 'disabled' })
  await setLook('de', 'light', 760, 540)
  await main.getByTestId('lightbox-close').scrollIntoViewIfNeeded()
  await main.screenshot({ path: join(shots, 'desktop-files-markdown-de-light-760.png'), animations: 'disabled' })

  // Escape closes lightbox
  await main.keyboard.press('Escape')
  await lightbox.waitFor({ state: 'detached' })

  // 2. Channels workspace fullscreen preview
  await setLook('zh', 'light', 1180, 760)
  await main.evaluate(() => { location.hash = '/channels' })
  await main.getByTestId('channel-item').first().click()
  await main.getByTestId('message-attachment-open').first().waitFor()
  await main.getByTestId('message-attachment-open').first().click()

  await lightbox.waitFor()
  await main.getByTestId('lightbox-image').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), 'channel-mockup.png')
  assert.equal(await main.getByTestId('lightbox-counter').innerText(), '1 / 3')
  assert.ok(calls.some((c) => c.method === 'channel/attachments'), 'channel/attachments should be queried')
  await main.screenshot({ path: join(shots, 'desktop-channel-image-zh-light-1180.png'), animations: 'disabled' })

  await main.getByTestId('lightbox-next').click()
  await main.getByTestId('lightbox-markdown').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), 'review-notes.md')
  await setLook('zh', 'dark', 1180, 760)
  await main.screenshot({ path: join(shots, 'desktop-channel-markdown-zh-dark-1180.png'), animations: 'disabled' })

  await main.getByTestId('lightbox-next').click()
  await main.getByTestId('lightbox-fallback').waitFor()
  assert.equal(await main.getByTestId('lightbox-filename').innerText(), 'archive-bundle.pdf')

  // A synced deletion removes the current attachment even while its old message is still visible.
  channelAttachments.pop()
  for (const socket of sockets) socket.send(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { seq: 1, type: 'channel/changed', data: {} } }))
  await main.getByTestId('lightbox-image').waitFor()
  assert.equal(await main.getByTestId('lightbox-counter').innerText(), '1 / 2')

  await main.keyboard.press('Escape')
  await lightbox.waitFor({ state: 'detached' })

  assert.ok(previewRequests.length >= 6, `expected >= 6 preview HTTP fetches, got ${previewRequests.length}`)
  assert.deepEqual(failures, [])
  console.log(`Desktop fullscreen preview Electron checks passed (${previewRequests.length} previews); screenshots: ${shots}`)
} catch (error) {
  const page = await app.firstWindow()
  await page.screenshot({ path: join(shots, 'failure.png') })
  console.log('Recent RPC calls:', calls.slice(-15).map((c) => c.method))
  console.log('Preview HTTP requests:', previewRequests)
  throw error
} finally {
  await app.close()
  for (const socket of sockets) socket.terminate()
  wss.close()
  await new Promise((resolve) => server.close(resolve))
  await rm(profile, { recursive: true, force: true })
}
