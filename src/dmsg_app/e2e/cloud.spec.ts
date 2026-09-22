import { chromium, expect, test } from '@playwright/test'
import { build } from 'vite'
import { spawn, type ChildProcess } from 'node:child_process'
import { cp, mkdtemp, readFile, writeFile, rm } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

test('MV3 → real user Wasm → workerd signed profile and negative trust checks', async ({}, testInfo) => {
  test.skip(
    !process.env.DMSG_CLOUD_DIR,
    'Set DMSG_CLOUD_DIR for the private local relay harness'
  )
  test.setTimeout(300000)
  const app = resolve('.'),
    root = resolve('../..'),
    cloud = resolve(process.env.DMSG_CLOUD_DIR!)
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-cloud-probe-'))
  const extension = join(dir, 'extension'),
    children: ChildProcess[] = []
  let browser: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | undefined
  const logs: string[] = []
  function start(command: string, args: string[], cwd: string) {
    const child = spawn(command, args, {
      cwd,
      env: { ...process.env, DMSG_CLOUD_FIXTURE_DIR: dir, WRANGLER_SEND_METRICS: 'false' },
      stdio: ['ignore', 'pipe', 'pipe']
    })
    child.stdout!.on('data', (data) => logs.push(String(data)))
    child.stderr!.on('data', (data) => logs.push(String(data)))
    child.on('error', (error) => logs.push(error.message))
    children.push(child)
    return child
  }
  async function readWhenReady(file: string, child: ChildProcess) {
    const end = Date.now() + 120000
    while (Date.now() < end) {
      try {
        return JSON.parse(await readFile(join(dir, file), 'utf8'))
      } catch {
        /* atomic file publication */
      }
      if (child.exitCode !== null) throw new Error(logs.join(''))
      await new Promise((done) => setTimeout(done, 100))
    }
    throw new Error(`Timeout waiting for ${file}\n${logs.join('')}`)
  }
  try {
    const gateway = start(
      'cargo',
      [
        'test',
        '--locked',
        '-p',
        'dmsg_integration',
        '--features',
        'pocketic-tests',
        '--test',
        'cloud_fixture',
        '--',
        '--ignored',
        '--nocapture'
      ],
      root
    )
    const fixture = await readWhenReady('fixture.json', gateway)
    await cp(resolve('dist'), extension, { recursive: true })
    await build({
      configFile: false,
      root: app,
      publicDir: false,
      logLevel: 'warn',
      build: {
        target: 'chrome120',
        outDir: extension,
        emptyOutDir: false,
        rollupOptions: { input: resolve('e2e/cloud-probe.html') }
      }
    })
    // Only this disposable test build can contact arbitrary loopback test ports.
    const manifestPath = join(extension, 'manifest.json')
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'))
    manifest.host_permissions.push('http://127.0.0.1/*', 'http://localhost/*')
    manifest.content_security_policy.extension_pages =
      manifest.content_security_policy.extension_pages.replace(
        "connect-src 'self'",
        "connect-src 'self' http://127.0.0.1:* http://localhost:*"
      )
    await writeFile(manifestPath, JSON.stringify(manifest))
    browser = await chromium.launchPersistentContext(join(dir, 'chrome'), {
      headless: true,
      ignoreDefaultArgs: ['--disable-extensions'],
      ...(process.env.DMSG_TEST_CHROME
        ? { executablePath: process.env.DMSG_TEST_CHROME }
        : { channel: 'chromium' }),
      args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
    })
    const worker = browser.serviceWorkers()[0] || (await browser.waitForEvent('serviceworker'))
    const extensionOrigin = `chrome-extension://${new URL(worker.url()).host}`
    const port = await new Promise<number>((done, reject) => {
      const socket = createServer()
      socket.once('error', reject)
      socket.listen(0, '127.0.0.1', () => {
        const address = socket.address()
        if (!address || typeof address === 'string') return reject(new Error('No port'))
        socket.close(() => done(address.port))
      })
    })
    await writeFile(
      join(dir, 'cloud-launch.json'),
      JSON.stringify({ ...fixture, extensionOrigin, port })
    )
    const relay = start(
      process.execPath,
      [join(cloud, 'dmsg-core/scripts/extension-probe-server.mjs')],
      cloud
    )
    const { relay: origin } = await readWhenReady('cloud-ready.json', relay)
    const page = await browser.newPage()
    await page.goto(`${extensionOrigin}/e2e/cloud-probe.html`)
    await page.waitForFunction(() => 'cloudProbe' in window)
    const results = await page.evaluate((input) => (window as any).cloudProbe(input), {
      ...fixture,
      relay: origin
    })
    expect(results.readiness).toMatchObject({ protocol: 'dmsg-cloud/1', ready: false })
    expect(results.written).toEqual(results.retry)
    expect(results.profile).toEqual(results.finalProfile)
    expect(results.profile.display_name).toBe('P0 local integration')
    for (const name of [
      'wrongCanister',
      'wrongAccount',
      'staleEvidence',
      'modifiedDevice',
      'badCertificate',
      'tamperedCommand',
      'wrongProtocol',
      'wrongDeviceKey',
      'wrongResource'
    ])
      expect(results[name]).toBeTruthy()
    expect(results.tamperedBodyStatus).toBe(401)
    await writeFile(testInfo.outputPath('cloud-result.json'), JSON.stringify(results, null, 2))
  } finally {
    await writeFile(join(dir, 'stop'), '')
    await browser?.close()
    for (const child of [...children].reverse()) {
      if (child.exitCode !== null || child.signalCode !== null) continue
      // Give the Rust fixture time to read its stop file and drop PocketIC.
      // Killing cargo immediately can orphan its still-running test executable.
      if (child !== children[0]) child.kill('SIGTERM')
      await new Promise<void>((done) => {
        const terminate = setTimeout(() => child.kill('SIGTERM'), 2000)
        const force = setTimeout(() => child.kill('SIGKILL'), 5000)
        child.once('exit', () => {
          clearTimeout(terminate)
          clearTimeout(force)
          done()
        })
      })
    }
    await writeFile(testInfo.outputPath('cloud-processes.log'), logs.join(''))
    await rm(dir, { recursive: true, force: true })
  }
})
