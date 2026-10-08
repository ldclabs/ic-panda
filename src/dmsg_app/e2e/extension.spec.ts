import { chromium, expect, test, type BrowserContext, type Page } from '@playwright/test'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const extension = resolve('dist')
async function launch() {
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-extension-test-'))
  const context = await chromium.launchPersistentContext(dir, {
    headless: true,
    ignoreDefaultArgs: ['--disable-extensions'],
    ...(process.env.DMSG_TEST_CHROME
      ? { executablePath: process.env.DMSG_TEST_CHROME }
      : { channel: 'chromium' }),
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
    viewport: { width: 1440, height: 1000 }
  })
  const worker = context.serviceWorkers()[0] || (await context.waitForEvent('serviceworker'))
  const origin = `chrome-extension://${new URL(worker.url()).host}`
  return { context, origin, dir }
}
async function noOverflow(page: Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
    true
  )
}
test('loads the actual MV3 package, creates a provisional workspace and resumes its binding without a password', async ({}, testInfo) => {
  const first = await launch(),
    contexts: BrowserContext[] = [first.context],
    dirs = [first.dir]
  const errors: string[] = []
  try {
    const page = await first.context.newPage()
    page.on('pageerror', (error) => errors.push(error.message))
    await page.goto(`${first.origin}/index.html`)
    await expect(page.getByRole('button', { name: '创建我的工作台' })).toBeVisible()
    await page.screenshot({ path: testInfo.outputPath('welcome-desktop.png'), fullPage: true })
    await page.getByRole('button', { name: '创建我的工作台' }).click()
    await expect(page.getByRole('heading', { name: '登录，绑定你的账户。' })).toBeVisible({
      timeout: 30000
    })
    // The published package carries no service IDs, so binding stays disabled
    // instead of contacting anything.
    await expect(page.getByRole('button', { name: /连接 Internet Identity/ })).toBeDisabled()
    await page.screenshot({ path: testInfo.outputPath('bind-desktop.png'), fullPage: true })
    await page.setViewportSize({ width: 390, height: 844 })
    await noOverflow(page)
    await page.screenshot({ path: testInfo.outputPath('bind-390.png'), fullPage: true })
    await page.setViewportSize({ width: 320, height: 760 })
    await noOverflow(page)
    await page.setViewportSize({ width: 1440, height: 1000 })
    const storage = await page.evaluate(async () => {
      const names = await indexedDB.databases(),
        name = names.find((x) => x.name?.startsWith('dmsg:local:'))?.name
      if (!name) throw new Error('No workspace DB')
      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const open = indexedDB.open(name)
        open.onsuccess = () => resolve(open.result)
        open.onerror = () => reject(open.error)
      })
      const tx = db.transaction(['objects', 'key_envelopes', 'meta'])
      const read = (store: string) =>
        new Promise<any[]>((resolve) => {
          const request = tx.objectStore(store).getAll()
          request.onsuccess = () => resolve(request.result)
        })
      const [objects, envelopes, meta] = await Promise.all([
        read('objects'),
        read('key_envelopes'),
        read('meta')
      ])
      db.close()
      return {
        objects: objects.length,
        envelope: envelopes[0],
        unlock: meta.find((row) => row.id === 'workspace')?.value.unlock
      }
    })
    // Only device keys exist before the account: wrapped under a provisional
    // key, with no content, no password-derived material and no PRF copy.
    expect(storage.objects).toBe(0)
    expect(storage.unlock).toBe('provisional')
    expect(typeof storage.envelope.provisional).toBe('string')
    expect(storage.envelope.prfWrapped).toBeUndefined()
    // Reloading ends the dedicated worker; reopening resumes the setup.
    await page.reload()
    await expect(page.getByRole('heading', { name: '欢迎回到你的空间。' })).toBeVisible()
    await expect(page.getByText('这台设备尚未绑定账户。继续完成设置。')).toBeVisible()
    await page.getByRole('button', { name: '继续设置' }).click()
    await expect(page.getByRole('heading', { name: '登录，绑定你的账户。' })).toBeVisible()
    await page.getByRole('button', { name: '加入已有账户的新设备' }).click()
    await expect(page.getByRole('heading', { name: '加入已有账户。' })).toBeVisible()
    await page.getByRole('button', { name: '返回' }).click()
    await page.getByRole('button', { name: '所有设备丢失后恢复' }).click()
    await expect(page.getByRole('heading', { name: '所有设备都丢失时。' })).toBeVisible()
    await page.getByRole('button', { name: '返回' }).click()
    await expect(page.getByRole('heading', { name: '登录，绑定你的账户。' })).toBeVisible()
    // A service-worker restart must not disturb the unlocked page.
    const cdp = await first.context.newCDPSession(page)
    await cdp.send('ServiceWorker.enable')
    await cdp.send('ServiceWorker.stopAllWorkers')
    await expect(page.getByRole('heading', { name: '登录，绑定你的账户。' })).toBeVisible()
    await cdp.detach()
    const popup = await first.context.newPage()
    popup.on('pageerror', (error) => errors.push(error.message))
    await popup.setViewportSize({ width: 360, height: 640 })
    await popup.goto(`${first.origin}/popup.html`)
    await expect(popup.getByRole('button', { name: '打开工作台', exact: true })).toBeVisible()
    expect(
      await popup.evaluate(() =>
        performance
          .getEntriesByType('resource')
          .some((entry) => /\/assets\/(App-|[^/]*Settings-)/.test(entry.name))
      )
    ).toBe(false)
    await noOverflow(popup)
    await popup.screenshot({ path: testInfo.outputPath('popup.png'), fullPage: true })
    const panel = await first.context.newPage()
    panel.on('pageerror', (error) => errors.push(error.message))
    await panel.setViewportSize({ width: 390, height: 844 })
    await panel.goto(`${first.origin}/sidepanel.html`)
    await expect(panel.getByRole('heading', { name: '欢迎回到你的空间。' })).toBeVisible()
    await expect(panel.getByText('在这里继续会锁定那个窗口')).toBeVisible()
    await panel.getByRole('button', { name: '继续设置' }).click()
    await expect(panel.getByRole('heading', { name: '登录，绑定你的账户。' })).toBeVisible()
    // One page holds the unlocked workspace: the side panel took it over from the full page.
    await expect(page.getByRole('heading', { name: '欢迎回到你的空间。' })).toBeVisible()
    await noOverflow(panel)
    await panel.screenshot({ path: testInfo.outputPath('sidepanel.png'), fullPage: true })
    // A second blank browser starts from scratch: nothing to restore offline.
    const fresh = await launch()
    contexts.push(fresh.context)
    dirs.push(fresh.dir)
    const blank = await fresh.context.newPage()
    blank.on('pageerror', (error) => errors.push(error.message))
    await blank.goto(`${fresh.origin}/index.html`)
    await expect(blank.getByRole('button', { name: '创建我的工作台' })).toBeVisible()
    expect(await blank.locator('body').innerText()).not.toContain('恢复包')
    expect(errors).toEqual([])
  } finally {
    for (const context of contexts) await context.close()
    for (const dir of dirs) await rm(dir, { recursive: true, force: true })
  }
})
