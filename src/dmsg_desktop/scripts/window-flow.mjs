import assert from 'node:assert/strict'
import { join } from 'node:path'

async function windowControls(mainWindow, page, fullScreen, shots) {
  const toggle = page.getByTestId('sidebar-toggle')
  const check = (expanded) => page.waitForFunction(({ expanded, fullScreen }) => {
    const rtl = document.documentElement.dir === 'rtl'
    const start = rtl || fullScreen ? 18 : 82
    const controls = ['sidebar-toggle', 'nav-back', 'nav-forward'].map((id) => document.querySelector(`[data-testid="${id}"]`))
    if (controls[0]?.getAttribute('aria-expanded') !== String(expanded)) return false
    return controls.every((control, index) => {
      if (!control) return false
      const { left, right, top, width, height } = control.getBoundingClientRect()
      const edge = rtl ? innerWidth - right : left
      const centerY = top + height / 2
      return Math.abs(edge - (start + index * 34)) < 1 && Math.abs(centerY - 29.5) < 1 &&
        control.contains(document.elementFromPoint(left + width / 2, centerY))
    })
  }, { expanded, fullScreen })

  if (!fullScreen) {
    assert.deepEqual(await mainWindow.evaluate((window) => window.getWindowButtonPosition()), { x: 18, y: 24 })
    await mainWindow.evaluate(async (window) => {
      window.focus()
      for (const event of ['minimize', 'restore']) {
        await new Promise((resolve, reject) => {
          const timer = setTimeout(() => reject(new Error(`Window did not reach ${event}`)), 10_000)
          window.once(event, () => { clearTimeout(timer); resolve() })
          window[event]()
        })
        if (window.isMinimized() !== (event === 'minimize')) throw new Error(`Window state did not match ${event}`)
      }
      window.focus()
    })
  }
  const from = await page.evaluate(() => location.hash)
  const to = from === '#/vault' ? '/channels' : '/vault'
  await page.evaluate((route) => { location.hash = route }, to)
  await page.getByTestId(to === '/vault' ? 'vault-list' : 'channel-list').waitFor()
  await page.getByTestId('nav-back').click()
  await page.waitForFunction((route) => location.hash === route, from)
  await page.getByTestId('nav-forward').click()
  await page.waitForFunction((route) => location.hash === `#${route}`, to)
  if (to !== '/vault') {
    await page.getByTestId('nav-back').click()
    await page.getByTestId('vault-list').waitFor()
  }

  const initial = await toggle.getAttribute('aria-expanded') === 'true'
  for (const [locale, html, theme] of [['zh', 'zh-Hans', 'light'], ['ar', 'ar', 'dark']]) {
    await page.evaluate(async ([locale, theme]) => { await window.dmsg.setLocale(locale); await window.dmsg.setAppearance(theme) }, [locale, theme])
    await page.waitForSelector(`html[lang="${html}"]`)
    await page.waitForFunction((dark) => matchMedia('(prefers-color-scheme: dark)').matches === dark, theme === 'dark')
    await check(initial)
    if (locale === 'ar') {
      assert.equal(await page.locator('header').first().evaluate((el) => getComputedStyle(el).paddingLeft), fullScreen ? '20px' : '84px')
    }
    await page.mouse.move(300, 100)
    await page.screenshot({ path: join(shots, `window-controls-${fullScreen ? 'fullscreen' : 'windowed'}-${locale}.png`), animations: 'disabled' })
    await toggle.click()
    await check(!initial)
    await toggle.press('Space')
    await check(initial)
  }
  await page.evaluate(async () => { await window.dmsg.setLocale('en'); await window.dmsg.setAppearance('light') })
  await page.waitForSelector('html[lang="en"]')
  if (!fullScreen) {
    const size = await mainWindow.evaluate((window) => window.getContentSize())
    await mainWindow.evaluate((window) => window.setContentSize(760, 540))
    await check(false)
    await toggle.click()
    await check(true)
    await mainWindow.evaluate((window, [width, height]) => window.setContentSize(width, height), size)
    await check(initial)
  }
}

// Closing the main window hides it. In macOS full screen it leaves full
// screen first, since hiding it there leaves its Space behind, black; a
// second close during the exit changes nothing, and opening the window
// again cancels the hide. Full screen hides the traffic lights, so the
// window's buttons move to the edge.
export async function windowLifecycle(app, main, shots, { checkControls = false } = {}) {
  const mainWindow = await app.browserWindow(main)
  const nativeFullscreen = process.platform === 'darwin' && process.env.DMSG_DESKTOP_NATIVE_FULLSCREEN !== '0'
  for (const mode of nativeFullscreen ? ['windowed', 'fullscreen', 'reopen-during-exit'] : ['windowed']) {
    const fullScreen = mode !== 'windowed'
    await app.evaluate(async ({ app }, [window, fullScreen]) => {
      // Native macOS Space transitions need a foreground application, unlike CDP clicks.
      app.focus({ steal: true })
      window.show()
      // Read the state: on macOS `show` follows occlusion, which a quick hide and show may never change.
      for (let waited = 0; !window.isVisible(); waited += 50) {
        if (waited >= 10_000) throw new Error('Window did not become visible')
        await new Promise((resolve) => setTimeout(resolve, 50))
      }
      if (!fullScreen) return
      window.focus()
      const entered = new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error('Native full screen requires dMsg in front and an unlocked macOS session')), 10_000)
        window.once('enter-full-screen', () => { clearTimeout(timer); resolve() })
      })
      window.setFullScreen(true)
      await entered
    }, [mainWindow, fullScreen])
    if (fullScreen) {
      await main.waitForSelector('html[data-fullscreen]')
      assert.equal((await main.locator('[data-testid="sidebar-toggle"]').boundingBox()).x, 18)
      if (mode === 'fullscreen') await main.screenshot({ path: join(shots, 'fullscreen.png') })
    }
    if (checkControls && process.platform === 'darwin' && mode !== 'reopen-during-exit') {
      await windowControls(mainWindow, main, fullScreen, shots)
    }
    const closed = await app.evaluate(
      async ({ app, Menu }, [window, mode]) => {
        const event = mode === 'windowed' ? 'hide' : 'leave-full-screen'
        let closes = 0
        const count = () => closes++
        window.on('close', count)
        let finished
        const done = new Promise((resolve, reject) => {
          const timer = setTimeout(() => reject(new Error(`Window did not reach ${event}`)), 10_000)
          finished = () => {
            clearTimeout(timer)
            window.removeListener(event, finished)
            resolve()
          }
          window.once(event, finished)
        })
        window.close()
        // macOS can hide synchronously without a separate hide notification.
        // The assertions below still require a live hidden window that reopens.
        if (mode === 'windowed' && !window.isDestroyed() && !window.isVisible()) finished()
        // A person's second close, by ⌘W or the close button, reaches `close` at once. A second
        // `window.close()` would wait out the transition in Electron, then hide the reopened window.
        if (mode !== 'windowed') Menu.sendActionToFirstResponder('performClose:')
        if (mode === 'reopen-during-exit') app.emit('activate')
        await done
        const result = { closes, hidden: !window.isVisible(), fullScreen: window.isFullScreen(), destroyed: window.isDestroyed() }
        window.removeListener('close', count)
        // The Dock brings back the same window and page.
        app.emit('activate')
        return { ...result, reopened: window.isVisible() }
      },
      [mainWindow, mode]
    )
    assert.deepEqual(closed, { closes: mode === 'windowed' ? 1 : 2, hidden: mode !== 'reopen-during-exit', fullScreen: false, destroyed: false, reopened: true }, `close and reopen (${mode})`)
    await main.waitForSelector('html:not([data-fullscreen])')
    if (checkControls && process.platform === 'darwin') {
      assert.equal((await main.getByTestId('sidebar-toggle').boundingBox()).x, 82)
    }
    // Nothing the close left pending may take back the reopened window.
    await new Promise((resolve) => setTimeout(resolve, 300))
    assert.equal(await mainWindow.evaluate((window) => window.isVisible()), true, `still open after close and reopen (${mode})`)
  }
  return nativeFullscreen
}
