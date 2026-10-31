import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, posix, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import MarkdownIt from 'markdown-it'

const frontend = fileURLToPath(new URL('..', import.meta.url))
const repository = resolve(frontend, '../..')
const origin = 'https://dmsg.net'
const github = 'https://github.com/ldclabs/ic-panda'
export const marker = '<!-- dmsg:public-content -->'

// Explicit public files only. Never discover documents from sibling repositories,
// environment variables, arbitrary directories or the private design baseline.
export const sources = [
  [
    'architecture',
    'Technical architecture and trust domains',
    'docs/dmsg_architecture.md'
  ],
  [
    'protocols',
    'Public protocols and signature formats',
    'docs/protocol/README.md'
  ],
  [
    'account',
    'Accounts, devices and recovery',
    'docs/protocol/account_root.md'
  ],
  ['cloud', 'Encrypted content, files and sharing', 'docs/protocol/cloud.md'],
  [
    'agents',
    'Agent connector, tools and capabilities',
    'src/dmsg_agent/README.md'
  ],
  [
    'agent-protocol',
    'Principal documents and Agent Delegation',
    'docs/protocol/agent.md'
  ],
  ['cli', 'CLI, installation and MCP configuration', 'src/dmsg_cli/README.md'],
  ['secrets', 'Approved credential use', 'docs/protocol/secrets.md'],
  ['wallet', 'Team wallets', 'docs/protocol/wallet.md'],
  ['sdk', 'Web application SDK', 'packages/dmsg-sdk/README.md'],
  ['connect', 'dmsg-connect/1', 'docs/protocol/connect-v1.md'],
  [
    'integration',
    'Authentication and application integration',
    'docs/protocol/integration.md'
  ],
  [
    'commerce',
    'Membership, checkout and entitlements',
    'docs/protocol/commerce.md'
  ]
].map(([id, title, path]) => ({
  id,
  title,
  path,
  url: `${origin}/_/docs/${id}.md`
}))

const sha256 = (text) => createHash('sha256').update(text).digest('hex')
const read = (path) => readFileSync(resolve(repository, path), 'utf8')
const escape = (value) =>
  String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;'
      })[c]
  )

export function rewriteLinks(markdown, sourcePath) {
  const link = (href) => {
    if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(href)) return href
    const [, path, suffix = ''] = href.match(/^([^?#]*)(.*)$/)
    const target = posix.normalize(posix.join(posix.dirname(sourcePath), path))
    const local = sources.find((source) => source.path === target)
    return local
      ? local.url + suffix
      : new URL(href, `${github}/blob/main/${sourcePath}`).href
  }
  // Leave fenced examples unchanged, including examples of Markdown syntax.
  let fence
  return markdown
    .split('\n')
    .map((line) => {
      const match = line.match(/^\s{0,3}(`{3,}|~{3,})/)
      if (match) {
        if (!fence) fence = match[1]
        else if (match[1][0] === fence[0] && match[1].length >= fence.length)
          fence = undefined
        return line
      }
      if (fence) return line
      return line
        .replace(
          /(\]\()([^\s)]+)(\))/g,
          (_, before, href, after) => before + link(href) + after
        )
        .replace(
          /^(\s{0,3}\[[^\]]+\]:\s*)(\S+)/,
          (_, before, href) => before + link(href)
        )
    })
    .join('\n')
}

function membershipTable(membership, zh) {
  const heading = zh
    ? '| 会员 | USD / 年 | 加密云存储 | 活跃设备 | 活跃频道 | 正式签名 / 月 | 钱包名额 |'
    : '| Plan | USD / year | Encrypted cloud storage | Active devices | Active channels | Formal signatures / month | Wallet slots |'
  return (
    `${zh ? '公开价格核对日期' : 'Public pricing checked'}: ${membership.checkedAt}\n\n${heading}\n| --- | ---: | --- | ---: | ---: | ---: | ---: |\n` +
    membership.plans
      .map(
        (plan) =>
          `| ${plan.name} | ${plan.usd} | ${plan.storage} ${plan.unit} | ${plan.devices} | ${plan.channels} | ${plan.signatures} | ${plan.wallets} |`
      )
      .join('\n')
  )
}

const md = new MarkdownIt({ html: false })
md.renderer.rules.link_open = (tokens, index, options, env, renderer) => {
  const token = tokens[index]
  const href = token.attrGet('href')
  if (href?.startsWith(origin + '/')) {
    const url = new URL(href)
    token.attrSet('href', url.pathname + url.search + url.hash)
  }
  return renderer.renderToken(tokens, index, options)
}
md.renderer.rules.table_open = () =>
  '<div class="reading-table" tabindex="0" role="region" aria-label="Membership / 会员"><table>\n'
md.renderer.rules.table_close = () => '</table></div>\n'

export function buildPublicContent(output = resolve(frontend, 'dist')) {
  const shellPath = join(output, 'index.html')
  const shell = readFileSync(shellPath, 'utf8')
  if (shell.split(marker).length !== 2)
    throw new Error('Expected one public-content marker in the built SPA shell')

  const membershipPath =
    'src/dmsg_frontend/src/lib/components/landing/membership.json'
  const membership = JSON.parse(read(membershipPath))
  const documents = sources.map((source) => {
    const original = read(source.path)
    return {
      ...source,
      sourceSha256: sha256(original),
      text: `# ${source.title}\n\nSource: [${source.path}](${github}/blob/main/${source.path}). This is a public-source snapshot; review dates and deployment limits below still apply.\n\n---\n\n${rewriteLinks(original, source.path)}`
    }
  })
  const guides = ['en', 'zh'].map((lang) => {
    const path = `src/dmsg_frontend/content/overview${lang === 'zh' ? '.zh' : ''}.md`
    const original = read(path)
    if (original.split('<!-- membership -->').length !== 2)
      throw new Error(`Missing membership table in ${path}`)
    return {
      lang,
      path,
      sourceSha256: sha256(original),
      url: `${origin}/index${lang === 'zh' ? '.zh' : ''}.md`,
      text: original.replace(
        '<!-- membership -->',
        membershipTable(membership, lang === 'zh')
      )
    }
  })
  const git = (...args) =>
    execFileSync('git', args, { cwd: repository, encoding: 'utf8' }).trim()
  const inputs = [
    ...sources.map(({ path }) => path),
    'src/dmsg_frontend/content',
    membershipPath,
    'src/dmsg_frontend/scripts/public-content.mjs'
  ]
  const manifest = {
    format: 'dmsg-public-knowledge/1',
    generatedAt: new Date().toISOString(),
    sourceRevision: git('rev-parse', 'HEAD'),
    sourceDirty: Boolean(git('status', '--porcelain', '--', ...inputs)),
    scope:
      'Public source snapshot, not production deployment or audit evidence. Generation time is not a document review date.',
    releases: `${github}/releases/latest`,
    pricingCheckedAt: membership.checkedAt,
    documents: [...guides, ...documents].map(
      ({ path, url, text, sourceSha256 }) => ({
        source: path,
        url,
        sourceSha256,
        sha256: sha256(text)
      })
    )
  }
  const write = (path, content) => {
    const target = join(output, path)
    mkdirSync(dirname(target), { recursive: true })
    writeFileSync(target, content)
  }
  const body = (guide) =>
    `<main class="reading-body">${md.render(guide.text)}<footer><a href="/_/knowledge.json">${guide.lang === 'zh' ? '构建依据与内容哈希' : 'Build provenance and content hashes'}</a> · <a href="/">dMsg</a></footer></main>`
  for (const guide of guides) {
    const title = guide.text.split('\n')[0].slice(2)
    const description = guide.text.split('\n\n')[1]
    const page = `/_/about${guide.lang === 'zh' ? '.zh' : ''}.html`
    write(new URL(guide.url).pathname, guide.text)
    write(
      page,
      `<!doctype html>
<html lang="${guide.lang}" dir="ltr"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escape(title)}</title><meta name="description" content="${escape(description)}">
<link rel="canonical" href="${origin}${page}">
<link rel="alternate" hreflang="en" href="${origin}/_/about.html">
<link rel="alternate" hreflang="zh-Hans" href="${origin}/_/about.zh.html">
<link rel="alternate" type="text/markdown" href="${guide.url}">
<link rel="describedby" href="/llms.txt" type="text/plain">
<link rel="icon" href="/_assets/dmsg/icon.svg"><link rel="stylesheet" href="/_assets/dmsg/reading.css">
</head><body class="dmsg-site dmsg-reading">${body(guide)}</body></html>\n`
    )
  }
  for (const document of documents)
    write(new URL(document.url).pathname, document.text)
  write(
    '_assets/dmsg/reading.css',
    read('src/dmsg_frontend/src/lib/components/landing/site.css') +
      '\n' +
      read('src/dmsg_frontend/content/reading.css')
  )
  const overview = `<section id="dmsg-public-content" class="dmsg-site dmsg-reading" lang="en" dir="ltr">${body(guides[0])}</section>`
  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'SoftwareApplication',
    name: 'dMsg',
    url: origin + '/',
    description: guides[0].text.split('\n\n')[1],
    applicationCategory: 'BusinessApplication',
    operatingSystem: ['macOS', 'Windows', 'Linux (CLI)'],
    publisher: {
      '@type': 'Organization',
      name: 'ICPanda DAO',
      url: 'https://panda.fans'
    },
    downloadUrl: `${github}/releases/latest`,
    featureList: [
      'Encrypted vault and files',
      'Private channels',
      'Agent Protocols and MCP',
      'Explicit signature approvals',
      'Team wallets'
    ]
  }
  // Visible HTML, not noscript or a bot-specific response. The SPA removes it
  // only after mounting successfully; it remains readable if JS cannot run.
  write(
    'index.html',
    shell
      .replace(marker, overview)
      .replace(
        '</head>',
        `<link id="dmsg-reading-style" rel="stylesheet" href="/_assets/dmsg/reading.css">\n<script type="application/ld+json">${JSON.stringify(structuredData).replaceAll('<', '\\u003c')}</script>\n</head>`
      )
  )
  write('_/knowledge.json', JSON.stringify(manifest, null, 2) + '\n')
  write(
    'llms.txt',
    `# dMsg\n\n> Private collaboration for people and agents, by ICPanda DAO. Encrypted personal workspace, channels, team wallets, public identity, explicit approvals and Agent Protocols over MCP.\n\nPublic information requires no login or JavaScript. Workspace access requires the owner's explicit capabilities. These documents are generated with the website from public source; source implementation, released binaries and deployed services can differ. Keep each document's review dates and limitations. Use tools/list for the installed MCP catalog.\n\n## Start here\n\n- [Product, installation, agent setup, pricing and trust boundaries](${origin}/index.md)\n- [简体中文产品与接入指南](${origin}/index.zh.md)\n- [Build revision, generation time and content hashes](${origin}/_/knowledge.json)\n- [Latest releases and checksums](${github}/releases/latest)\n\n## Public implementation and contracts\n\n${sources.map((source) => `- [${source.title}](${source.url})`).join('\n')}\n\n## Optional\n\n- [Complete English guide and public reference bundle](${origin}/llms-full.txt): Large file; prefer individual references for a focused question.\n- [Privacy policy](${origin}/_/privacy)\n- [Terms of service](${origin}/_/terms/v1.txt)\n- [Source repository](${github})\n`
  )
  write(
    'llms-full.txt',
    [guides[0].text, ...documents.map(({ text }) => text)].join('\n\n---\n\n')
  )
  write(
    'robots.txt',
    `User-agent: *\nAllow: /\nDisallow: /legacy\nDisallow: /_/login\nDisallow: /_/connect\nDisallow: /_/channel\nDisallow: /_/setting\nDisallow: /s/\n\nSitemap: ${origin}/sitemap.xml\n`
  )
  // List real HTML documents only. Do not advertise dynamic profiles or
  // login/share URLs whose unauthenticated response is the generic SPA shell.
  write(
    'sitemap.xml',
    `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${['/', '/_/about.html', '/_/about.zh.html'].map((path) => `  <url><loc>${origin}${path}</loc></url>`).join('\n')}\n</urlset>\n`
  )
  return manifest
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const manifest = buildPublicContent()
  console.log(
    `Built public reading pages and ${manifest.documents.length} Markdown documents (${manifest.sourceRevision.slice(0, 8)}${manifest.sourceDirty ? ', working tree' : ''}).`
  )
}
