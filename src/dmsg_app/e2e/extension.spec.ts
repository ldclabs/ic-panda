import { chromium, expect, test, type BrowserContext, type Page } from '@playwright/test'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const extension = resolve('dist'),
  password = 'private-river-paper-2026'
async function launch() {
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-extension-test-'))
  const context = await chromium.launchPersistentContext(dir, {
    headless: true,
    ignoreDefaultArgs: ['--disable-extensions'],
    ...(process.env.DMSG_TEST_CHROME
      ? { executablePath: process.env.DMSG_TEST_CHROME }
      : { channel: 'chromium' }),
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`],
    viewport: { width: 1440, height: 1000 },
    acceptDownloads: true
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
test('loads the actual MV3 package, saves encrypted data, locks and recovers on a blank browser', async ({}, testInfo) => {
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
    await page.getByLabel('设置本机口令', { exact: true }).fill(password)
    await page.getByLabel('再次输入口令', { exact: true }).fill(password)
    await page.getByRole('button', { name: '建立加密工作台' }).click()
    await expect(page.locator('.recovery-code code')).toBeVisible({ timeout: 30000 })
    const recoveryCode = await page.locator('.recovery-code code').innerText()
    await page.reload()
    await expect(page.getByRole('heading', { name: '欢迎回到你的空间。' })).toBeVisible()
    await page.getByLabel('本机解锁口令').fill(password)
    await page.getByRole('button', { name: '解锁工作台', exact: true }).click()
    await expect(page.locator('.recovery-code code')).toHaveText(recoveryCode, {
      timeout: 30000
    })
    const initialDownload = page.waitForEvent('download')
    await page.getByRole('button', { name: '下载初始恢复包' }).click()
    await (await initialDownload).saveAs(testInfo.outputPath('initial.dmsg'))
    await page.getByLabel('我已确认下载文件，并另行保存恢复码').check()
    await page.getByRole('button', { name: '验证恢复码', exact: true }).click()
    await page.getByLabel('输入已保存的恢复码').fill(recoveryCode)
    await page.getByRole('button', { name: '验证并进入工作台' }).click()
    await expect(page.getByRole('heading', { name: '秘密库' })).toBeVisible()
    await page.getByRole('button', { name: '新建条目', exact: true }).click()
    await page.getByLabel('类型', { exact: true }).selectOption('api')
    await page.getByLabel('标题', { exact: true }).fill('Production deployment credential')
    await page.getByLabel('API Secret / Token').fill('never-persist-this-token-in-clear')
    await page.getByLabel('备注', { exact: true }).fill('private-note-body')
    await page.getByRole('button', { name: '加密保存', exact: true }).click()
    await expect(
      page.getByRole('heading', { name: 'Production deployment credential' })
    ).toBeVisible()
    const cdp = await first.context.newCDPSession(page)
    await cdp.send('ServiceWorker.enable')
    await cdp.send('ServiceWorker.stopAllWorkers')
    await expect(page.getByRole('heading', { name: '秘密库' })).toBeVisible()
    await cdp.detach()
    await expect(
      page.getByText('never-persist-this-token-in-clear', { exact: true })
    ).not.toBeVisible()
    await page.getByRole('button', { name: '显示私密字段', exact: true }).click()
    await page.getByRole('button', { name: '显示内容', exact: true }).click()
    await expect(
      page.getByText('never-persist-this-token-in-clear', { exact: true })
    ).toBeVisible()
    await page.getByRole('button', { name: '隐藏私密字段', exact: true }).click()
    await page.screenshot({ path: testInfo.outputPath('vault-desktop.png'), fullPage: true })
    await page.setViewportSize({ width: 390, height: 844 })
    await noOverflow(page)
    await page.screenshot({ path: testInfo.outputPath('vault-390.png'), fullPage: true })
    await page.setViewportSize({ width: 320, height: 760 })
    await noOverflow(page)
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.getByLabel('选择私密文件').setInputFiles({
      name: 'private-file.txt',
      mimeType: 'text/plain',
      buffer: Buffer.from('A private file with exact bytes.')
    })
    await expect(
      page.getByRole('heading', { name: 'private-file.txt', exact: true })
    ).toBeVisible()
    const fileDownload = page.waitForEvent('download')
    await page.getByRole('button', { name: '验证并下载文件' }).click()
    const downloadedFile = testInfo.outputPath('private-file.txt')
    await (await fileDownload).saveAs(downloadedFile)
    expect(await readFile(downloadedFile, 'utf8')).toBe('A private file with exact bytes.')
    await page.getByRole('button', { name: '立即锁定工作台', exact: true }).click()
    await expect(page.getByRole('heading', { name: '欢迎回到你的空间。' })).toBeVisible()
    expect(await page.locator('body').innerText()).not.toContain(
      'Production deployment credential'
    )
    await page.getByLabel('本机解锁口令').fill(password)
    await page.getByRole('button', { name: '解锁工作台', exact: true }).click()
    await expect(page.getByRole('heading', { name: '秘密库' })).toBeVisible()
    const storageDump = await page.evaluate(async () => {
      const names = await indexedDB.databases(),
        name = names.find((x) => x.name?.startsWith('dmsg:local:'))?.name
      if (!name) throw new Error('No workspace DB')
      const db = await new Promise<IDBDatabase>((resolve, reject) => {
        const open = indexedDB.open(name)
        open.onsuccess = () => resolve(open.result)
        open.onerror = () => reject(open.error)
      })
      const tx = db.transaction(['objects', 'outbox', 'local_private', 'key_envelopes'])
      const values = await Promise.all(
        ['objects', 'outbox', 'local_private', 'key_envelopes'].map(
          (store) =>
            new Promise((resolve) => {
              const request = tx.objectStore(store).getAll()
              request.onsuccess = () => resolve(request.result)
            })
        )
      )
      db.close()
      return JSON.stringify(values)
    })
    for (const privateText of [
      'Production deployment credential',
      'never-persist-this-token-in-clear',
      'private-file.txt',
      recoveryCode,
      password
    ])
      expect(storageDump).not.toContain(privateText)
    await page.reload()
    await expect(page.getByRole('heading', { name: '欢迎回到你的空间。' })).toBeVisible()
    await page.getByLabel('本机解锁口令').fill(password)
    await page.getByRole('button', { name: '解锁工作台', exact: true }).click()
    await expect(page.getByRole('heading', { name: '秘密库' })).toBeVisible()
    await page.getByRole('button', { name: '身份', exact: true }).click()
    await page.getByLabel('显示名', { exact: true }).fill('Private profile')
    await page.getByRole('button', { name: '保存资料草稿', exact: true }).click()
    await expect(page.getByText('资料草稿已加密保存，尚未公开发布。')).toBeVisible()
    await page.getByRole('button', { name: '立即锁定工作台', exact: true }).click()
    await page.getByLabel('本机解锁口令').fill(password)
    await page.getByRole('button', { name: '解锁工作台', exact: true }).click()
    await expect(page.getByLabel('显示名', { exact: true })).toHaveValue('Private profile')
    await page.getByRole('button', { name: '消息', exact: true }).click()
    await page.getByRole('button', { name: '新建会话', exact: true }).click()
    await page.getByLabel('会话名称', { exact: true }).fill('A private team draft')
    await page.getByLabel('对方稳定主体 ID', { exact: true }).fill('ab'.repeat(32))
    await page.getByRole('button', { name: '建立草稿', exact: true }).click()
    await expect(page.getByRole('heading', { name: 'A private team draft' })).toBeVisible()
    await page.getByLabel('消息正文').fill('Queued only, no fake ACK.')
    await page.getByRole('button', { name: '保存消息草稿', exact: true }).click()
    await expect(page.getByText('本地加密草稿 · 未投递', { exact: true })).toBeVisible()
    await page.getByRole('button', { name: '设置', exact: true }).click()
    await page.getByRole('button', { name: '导出加密备份' }).click()
    await page.getByLabel('当前口令').fill(password)
    const backupDownload = page.waitForEvent('download')
    await page.getByRole('button', { name: '生成加密备份', exact: true }).click()
    const backupPath = testInfo.outputPath('complete.dmsg')
    await (await backupDownload).saveAs(backupPath)
    const backup = JSON.parse(await readFile(backupPath, 'utf8'))
    expect(backup.missing).toEqual([])
    const fresh = await launch()
    contexts.push(fresh.context)
    dirs.push(fresh.dir)
    const recovery = await fresh.context.newPage()
    recovery.on('pageerror', (error) => errors.push(error.message))
    await recovery.goto(`${fresh.origin}/recovery.html`)
    await recovery.getByRole('button', { name: '从加密备份恢复' }).click()
    await recovery.getByLabel('加密恢复包', { exact: true }).setInputFiles(backupPath)
    await recovery.getByLabel('恢复码', { exact: true }).fill(recoveryCode)
    await recovery.getByLabel('设置本机新口令').fill(password)
    await recovery.getByLabel('确认新口令').fill(password)
    await recovery.getByRole('button', { name: '验证并恢复内容', exact: true }).click()
    await expect(recovery.getByRole('heading', { name: '秘密库' })).toBeVisible({
      timeout: 30000
    })
    await expect(
      recovery.getByText('Production deployment credential', { exact: true })
    ).toBeVisible()
    await recovery.screenshot({
      path: testInfo.outputPath('restored-desktop.png'),
      fullPage: true
    })
    const popup = await fresh.context.newPage()
    await popup.setViewportSize({ width: 360, height: 640 })
    await popup.goto(`${fresh.origin}/popup.html`)
    await expect(popup.getByRole('button', { name: '打开工作台', exact: true })).toBeVisible()
    await noOverflow(popup)
    await popup.screenshot({ path: testInfo.outputPath('popup.png'), fullPage: true })
    await recovery.getByRole('button', { name: '立即锁定工作台', exact: true }).click()
    const panel = await fresh.context.newPage()
    panel.on('pageerror', (error) => errors.push(error.message))
    await panel.setViewportSize({ width: 390, height: 844 })
    await panel.goto(`${fresh.origin}/sidepanel.html`)
    await panel.getByLabel('本机解锁口令').fill(password)
    await panel.getByRole('button', { name: '解锁工作台', exact: true }).click()
    await expect(panel.getByRole('heading', { name: '签名与授权' })).toBeVisible()
    await noOverflow(panel)
    await panel.screenshot({ path: testInfo.outputPath('sidepanel.png'), fullPage: true })
    expect(errors).toEqual([])
  } finally {
    for (const context of contexts) await context.close()
    for (const dir of dirs) await rm(dir, { recursive: true, force: true })
  }
})
