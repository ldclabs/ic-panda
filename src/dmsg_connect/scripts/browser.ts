/**
 * The real-browser driver of the desktop tests: JSON lines in, JSON lines
 * out, like `headless.ts`, but the application page (e2e/app) and the
 * bridge pages run in Chromium, Firefox or WebKit through Playwright. One
 * process is one browser profile.
 *
 *   {"id":1,"cmd":"init","browser":"chromium","bridge":"http://localhost:5174"}
 *   {"id":2,"cmd":"pair","url":"http://localhost:5174/v1/pair#..."}
 *   {"id":3,"cmd":"open","origin":"http://localhost:5173"}       -> {"app":"a1"}
 *   {"id":4,"cmd":"connect","app":"a1"}                          -> {"methods":[...]} or error
 */
import { createInterface } from 'node:readline'
import { chromium, firefox, webkit, type Browser, type BrowserContext, type Page } from 'playwright'

let browser: Browser | null = null
let context: BrowserContext | null = null
let bridge = ''
let pairPage: Page | null = null
const apps = new Map<string, { page: Page; popup: Page | null }>()

class Failure extends Error {
  readonly code: string
  constructor(code: string, message = code) {
    super(message)
    this.code = code
  }
}

function app(id: string) {
  const a = apps.get(id)
  if (!a) throw new Failure('NOT_FOUND', `no app page ${id}`)
  return a
}

/** Run `window.app[name](...args)` in the page; its `{value}` or `{error}`. */
async function call(page: Page, name: string, ...args: unknown[]): Promise<unknown> {
  const outcome = (await page.evaluate(
    ([n, a]) => (window as any).app[n as string](...(a as unknown[])),
    [name, args] as const
  )) as { value?: unknown; error?: string }
  if (outcome.error) throw new Failure(outcome.error)
  return outcome.value
}

async function state(page: Page, states: string[], timeout: number) {
  await page.waitForFunction((s) => s.includes(document.querySelector('main')?.dataset.state ?? ''), states, { timeout })
  return page.evaluate(() => ({ ...document.querySelector('main')!.dataset }))
}

async function run(input: Record<string, any>): Promise<unknown> {
  switch (input.cmd) {
    case 'init': {
      const type = { chromium, firefox, webkit }[input.browser as 'chromium']
      if (!type) throw new Failure('INVALID_INPUT', `browser ${input.browser}`)
      browser = await type.launch({ headless: input.headless ?? true, executablePath: input.executable || undefined })
      context = await browser.newContext()
      bridge = input.bridge
      return { browser: input.browser, version: browser.version() }
    }
    case 'pair': {
      pairPage = await context!.newPage()
      await pairPage.goto(input.url)
      return state(pairPage, ['waiting', 'error'], 30_000)
    }
    case 'await_paired':
      return state(pairPage!, ['active', 'error'], input.timeout_ms ?? 30_000)
    case 'open': {
      const page = await context!.newPage()
      await page.goto(`${input.origin}/?bridge=${encodeURIComponent(bridge)}`)
      await page.waitForFunction(() => document.documentElement.dataset.ready === 'true')
      const id = `a${apps.size + 1}`
      apps.set(id, { page, popup: null })
      return { app: id }
    }
    case 'connect': {
      const a = app(input.app)
      // A real click: the SDK opens the bridge in this user gesture (§4.1).
      const [popup] = await Promise.all([a.page.waitForEvent('popup'), a.page.click('#connect')])
      a.popup = popup
      return { methods: await call(a.page, 'ready') }
    }
    case 'sign_document': {
      const a = app(input.app)
      const pending = call(a.page, 'signDocument', input.text, input.account, input.issuer)
      // Safari's storage rule (§4.1): a create asks for a click in the window first.
      const button = await a.popup?.waitForSelector('[data-testid=choose-continue]', { timeout: 2_000 }).catch(() => null)
      if (button) await button.click()
      return { ...((await pending) as object), prompted: !!button }
    }
    case 'get': {
      const a = app(input.app)
      const deadline = Date.now() + (input.wait_ms ?? 0)
      for (;;) {
        const value = (await call(a.page, 'getOperation', input.operation_id)) as { state: string }
        if (!input.until || input.until.includes(value.state) || Date.now() > deadline) return value
        await new Promise((r) => setTimeout(r, 500))
      }
    }
    case 'cancel':
      return call(app(input.app).page, 'cancelOperation', input.operation_id)
    case 'ack':
      return call(app(input.app).page, 'acknowledge', input.operation_id, input.result_digest)
    case 'popup': {
      // What the bridge window shows, and its `dmsg://` links.
      const popup = app(input.app).popup!
      return popup.evaluate(() => ({
        status: document.querySelector('[data-testid=status]')?.textContent ?? '',
        activity: document.querySelector('[data-testid=activity]')?.textContent ?? '',
        links: [...document.querySelectorAll('a')].map((a) => a.getAttribute('href'))
      }))
    }
    case 'close_popup': {
      const a = app(input.app)
      await a.popup?.close()
      await a.page.waitForFunction(() => !(window as any).app.open(), undefined, { timeout: 5_000 })
      return { open: false }
    }
    case 'headers': {
      // The response headers the bridge page is served with.
      const response = await context!.request.get(`${bridge}${input.path ?? '/v1/'}`)
      return response.headers()
    }
    default:
      throw new Failure('UNSUPPORTED_PROTOCOL')
  }
}

const lines = createInterface({ input: process.stdin })
for await (const line of lines) {
  if (!line.trim()) continue
  let input: Record<string, any> = {}
  try {
    input = JSON.parse(line)
    if (input.cmd === 'quit') {
      await (browser as Browser | null)?.close()
      browser = null
      process.stdout.write(JSON.stringify({ id: input.id, ok: true, value: null }) + '\n')
      break
    }
    const value = await run(input)
    process.stdout.write(JSON.stringify({ id: input.id, ok: true, value }) + '\n')
  } catch (error) {
    const code = error instanceof Failure ? error.code : 'UNAVAILABLE'
    process.stdout.write(
      JSON.stringify({ id: input.id, ok: false, error: code, message: error instanceof Error ? error.message : String(error) }) + '\n'
    )
  }
}
await (browser as Browser | null)?.close()
