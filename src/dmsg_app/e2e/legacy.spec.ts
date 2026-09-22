import { chromium, expect, test, type BrowserContext } from '@playwright/test'
import { build } from 'vite'
import { cp, mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

test('real MV3 worker imports a paired legacy archive and restores it in a blank offline browser', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-legacy-'))
  const extension = join(dir, 'extension'),
    contexts: BrowserContext[] = []
  try {
    await cp(resolve('dist'), extension, { recursive: true })
    await build({
      configFile: false,
      root: resolve('.'),
      publicDir: false,
      logLevel: 'warn',
      worker: { format: 'es' },
      build: {
        target: 'chrome120',
        outDir: extension,
        emptyOutDir: false,
        rollupOptions: { input: resolve('e2e/legacy-probe.html') }
      }
    })
    for (const name of ['original', 'blank'])
      contexts.push(
        await chromium.launchPersistentContext(join(dir, name), {
          headless: true,
          ignoreDefaultArgs: ['--disable-extensions'],
          ...(process.env.DMSG_TEST_CHROME
            ? { executablePath: process.env.DMSG_TEST_CHROME }
            : { channel: 'chromium' }),
          args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
        })
      )
    const first = contexts[0]!,
      second = contexts[1]!
    const worker = first.serviceWorkers()[0] || (await first.waitForEvent('serviceworker'))
    const origin = `chrome-extension://${new URL(worker.url()).host}`
    const page = await first.newPage()
    await page.goto(`${origin}/e2e/legacy-probe.html`)
    await page.waitForFunction(() => 'legacyProbe' in window)
    const fixture = await page.evaluate(() => (window as any).legacyProbe())
    expect(fixture.idempotent).toBe(true)
    expect(fixture.message).toBe('synthetic browser legacy history')
    await first.close()
    await second.setOffline(true)
    const recovery = await second.newPage()
    await recovery.goto(`${origin}/e2e/legacy-probe.html`)
    await recovery.waitForFunction(() => 'legacyRestore' in window)
    const result = await recovery.evaluate(
      (input) => (window as any).legacyRestore(input),
      fixture
    )
    expect(result).toEqual({
      message: 'synthetic browser legacy history',
      stage: 'content_verified',
      snapshot: 'pre_migration',
      pairs: 0,
      visibleFiles: 0,
      registered: false
    })
  } finally {
    for (const context of contexts) await context.close()
    await rm(dir, { recursive: true, force: true })
  }
})
