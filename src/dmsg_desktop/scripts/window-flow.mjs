import assert from 'node:assert/strict'
import { join } from 'node:path'

// Closing the main window hides it. In macOS full screen it leaves full
// screen first, since hiding it there leaves its Space behind, black; a
// second close during the exit changes nothing, and opening the window
// again cancels the hide. Full screen hides the traffic lights, so the
// window's buttons move to the edge.
export async function windowLifecycle(app, main, shots) {
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
    // Nothing the close left pending may take back the reopened window.
    await new Promise((resolve) => setTimeout(resolve, 300))
    assert.equal(await mainWindow.evaluate((window) => window.isVisible()), true, `still open after close and reopen (${mode})`)
  }
  return nativeFullscreen
}
