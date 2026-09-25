import { chromium, expect, test } from '@playwright/test'
import { cp, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { execFileSync } from 'node:child_process'
import { browserProofMessage } from '../../../packages/dmsg-sdk/src/browser'

test('real external ports bind top-level documents and require a fresh session proof after reload', async () => {
  const original = resolve('.'),
    dir = await mkdtemp(join(tmpdir(), 'dmsg-external-v4-'))
  let context: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | undefined
  try {
    for (const file of [
      'src',
      'public',
      'index.html',
      'popup.html',
      'sidepanel.html',
      'approve.html',
      'recovery.html',
      'vite.config.ts',
      'package.json',
      'dmsg.config.json'
    ])
      await cp(join(original, file), join(dir, file), { recursive: true })
    await symlink(join(original, 'node_modules'), join(dir, 'node_modules'), 'dir')
    const config = JSON.parse(await readFile(join(dir, 'dmsg.config.json'), 'utf8'))
    config.externalOrigins = ['https://product.test', 'http://127.0.0.1:5188']
    await writeFile(join(dir, 'dmsg.config.json'), JSON.stringify(config))
    execFileSync(
      process.execPath,
      [join(original, 'node_modules/vite/bin/vite.js'), 'build'],
      { cwd: dir, stdio: 'pipe' }
    )
    const extension = join(dir, 'dist')
    context = await chromium.launchPersistentContext(join(dir, 'profile'), {
      headless: true,
      ignoreDefaultArgs: ['--disable-extensions'],
      ...(process.env.DMSG_TEST_CHROME
        ? { executablePath: process.env.DMSG_TEST_CHROME }
        : { channel: 'chromium' }),
      args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
    })
    const worker = context.serviceWorkers()[0] || (await context.waitForEvent('serviceworker'))
    const extensionId = new URL(worker.url()).host
    await context.route('https://product.test/**', (route) =>
      route.fulfill({
        contentType: 'text/html',
        body: '<!doctype html><title>Reference product</title>'
      })
    )
    const page = await context.newPage()
    await page.exposeFunction('proofMessage', async (command: any, challenge: any) =>
      Array.from(browserProofMessage(command, challenge.nonce, challenge))
    )
    await page.goto('https://product.test/')
    const run = (bad = false) =>
      page.evaluate(
        async ({ extensionId, bad }) => {
          const db = await new Promise<IDBDatabase>((resolve, reject) => {
            const r = indexedDB.open('product-session', 1)
            r.onupgradeneeded = () => r.result.createObjectStore('key')
            r.onsuccess = () => resolve(r.result)
            r.onerror = () => reject(r.error)
          })
          let pair = await new Promise<CryptoKeyPair | undefined>((resolve) => {
            const r = db.transaction('key').objectStore('key').get('session')
            r.onsuccess = () => resolve(r.result)
          })
          if (!pair) {
            pair = await crypto.subtle.generateKey(
              { name: 'ECDSA', namedCurve: 'P-256' },
              false,
              ['sign', 'verify']
            )
            const tx = db.transaction('key', 'readwrite')
            tx.objectStore('key').put(pair, 'session')
            await new Promise<void>((resolve) => {
              tx.oncomplete = () => resolve()
            })
          }
          db.close()
          const b64 = (value: Uint8Array) => btoa(String.fromCharCode(...value))
          const publicKey = b64(
            new Uint8Array(await crypto.subtle.exportKey('spki', pair.publicKey))
          )
          const command = {
            method: 'getOperation',
            appId: 'product',
            operationId: '01'.repeat(32),
            publicKey,
            payload: null,
            resultDigest: null
          }
          const port = chrome.runtime.connect(extensionId, { name: 'dmsg-extension/4' })
          return new Promise<{ error: string; documentId: string; publicKey: string }>(
            (resolve, reject) => {
              const timer = setTimeout(() => reject(new Error('port timeout')), 15000)
              let documentId = ''
              port.onMessage.addListener((message) => {
                void (async () => {
                  if (message.type === 'challenge') {
                    documentId = message.documentId
                    const bytes = new Uint8Array(
                      await (window as any).proofMessage(command, message)
                    )
                    const signature = bad
                      ? new Uint8Array(64)
                      : new Uint8Array(
                          await crypto.subtle.sign(
                            { name: 'ECDSA', hash: 'SHA-256' },
                            pair!.privateKey,
                            bytes
                          )
                        )
                    port.postMessage({
                      protocol: 'dmsg-extension/4',
                      requestId: message.requestId,
                      type: 'proof',
                      signature: b64(signature)
                    })
                  } else if (message.type === 'result') {
                    clearTimeout(timer)
                    resolve({ error: message.error, documentId, publicKey })
                    port.disconnect()
                  }
                })().catch(reject)
              })
              port.postMessage({
                protocol: 'dmsg-extension/4',
                requestId: '02'.repeat(32),
                type: 'request',
                command
              })
            }
          )
        },
        { extensionId, bad }
      )
    const first = await run()
    expect(first.error).toBe('LOCKED') // Valid proof reached the deliberately empty workspace.
    await page.reload()
    const second = await run()
    expect(second.error).toBe('LOCKED')
    expect(second.publicKey).toBe(first.publicKey)
    expect(second.documentId).not.toBe(first.documentId)
    expect((await run(true)).error).toBe('FORBIDDEN')
    await context.route('http://127.0.0.1:5188/**', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>Local product</title>' }))
    await page.goto('http://127.0.0.1:5188/')
    expect((await run()).error).toBe('LOCKED')
    await page.goto('https://product.test/')
    await page.setContent('<iframe src="https://product.test/frame"></iframe>')
    const frame = page.frames().find((f) => f.parentFrame())!
    await frame.waitForLoadState()
    expect(
      await frame.evaluate(
        (extensionId) =>
          new Promise<boolean>((resolve) => {
            const port = chrome.runtime.connect(extensionId, { name: 'dmsg-extension/4' })
            port.onDisconnect.addListener(() => resolve(true))
          }),
        extensionId
      )
    ).toBe(true)
  } finally {
    await context?.close()
    await rm(dir, { recursive: true, force: true })
  }
})
