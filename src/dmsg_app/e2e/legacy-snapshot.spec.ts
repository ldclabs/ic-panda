import { chromium, expect, test } from '@playwright/test'
import { build } from 'vite'
import { promisify } from 'node:util'
import { spawn, execFile } from 'node:child_process'
import { cp, mkdtemp, readFile, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

test('upgrades matched legacy releases and verifies frozen snapshot ingress proofs', async ({}, testInfo) => {
  test.skip(
    !process.env.DMSG_LEGACY_RELEASES,
    'Requires the verified I0 public release artifacts'
  )
  test.setTimeout(240000)
  const dir = await mkdtemp(join(tmpdir(), 'dmsg-legacy-snapshot-')),
    extension = join(dir, 'extension')
  const logs: string[] = []
  const gateway = spawn(
    'cargo',
    [
      'test',
      '--locked',
      '-p',
      'dmsg_integration',
      '--features',
      'pocketic-tests',
      '--test',
      'legacy_fixture',
      '--',
      '--ignored',
      '--nocapture'
    ],
    {
      cwd: resolve('../..'),
      env: { ...process.env, DMSG_CLOUD_FIXTURE_DIR: dir },
      stdio: ['ignore', 'pipe', 'pipe']
    }
  )
  gateway.stdout.on('data', (data) => logs.push(String(data)))
  gateway.stderr.on('data', (data) => logs.push(String(data)))
  let browser: Awaited<ReturnType<typeof chromium.launchPersistentContext>> | null = null
  try {
    let fixture: any = null
    for (let i = 0; i < 900; i++) {
      try {
        fixture = JSON.parse(await readFile(join(dir, 'fixture.json'), 'utf8'))
        break
      } catch {}
      if (gateway.exitCode !== null) throw new Error(logs.join(''))
      await new Promise((done) => setTimeout(done, 100))
    }
    if (!fixture) throw new Error(`Legacy fixture timeout\n${logs.join('')}`)
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
    const path = join(extension, 'manifest.json'),
      manifest = JSON.parse(await readFile(path, 'utf8'))
    manifest.host_permissions.push('http://127.0.0.1/*', 'http://localhost/*')
    manifest.content_security_policy.extension_pages =
      manifest.content_security_policy.extension_pages.replace(
        "connect-src 'self'",
        "connect-src 'self' http://127.0.0.1:* http://localhost:*"
      )
    await writeFile(path, JSON.stringify(manifest))
    browser = await chromium.launchPersistentContext(join(dir, 'browser'), {
      headless: true,
      ignoreDefaultArgs: ['--disable-extensions'],
      ...(process.env.DMSG_TEST_CHROME
        ? { executablePath: process.env.DMSG_TEST_CHROME }
        : { channel: 'chromium' }),
      args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
    })
    const worker = browser.serviceWorkers()[0] || (await browser.waitForEvent('serviceworker'))
    const page = await browser.newPage()
    await page.goto(`chrome-extension://${new URL(worker.url()).host}/e2e/legacy-probe.html`)
    await page.waitForFunction(() => 'legacySnapshotProbe' in window)
    const result = await page.evaluate(
      (input) => (window as any).legacySnapshotProbe(input),
      fixture
    )
    const operator = result.operator
    delete result.operator
    const inputPath = join(dir, 'name-proofs.json'),
      outputPath = join(dir, 'name-plan.json')
    await writeFile(
      inputPath,
      JSON.stringify({ names: operator.names, authorities: operator.authorities })
    )
    await promisify(execFile)(
      process.execPath,
      [
        resolve('scripts/legacy-name-plan.mjs'),
        '--input',
        inputPath,
        '--root-key-hex',
        operator.root,
        '--source',
        fixture.message,
        '--identity',
        fixture.identity,
        '--count',
        '1',
        '--cutover-hex',
        '08'.repeat(32),
        '--output',
        outputPath
      ],
      { timeout: 60000 }
    )
    const plan = JSON.parse(await readFile(outputPath, 'utf8'))
    expect(plan.calls.map((call: any) => call.method)).toEqual([
      'begin_legacy_snapshot',
      'import_legacy_handles',
      'seal_legacy_snapshot'
    ])
    expect(plan.count).toBe(1)
    expect(result).toEqual({
      frozenDeltaVerified: true,
      sharedConsentVerified: true,
      nameImportVerified: true,
      messages: fixture.messages,
      pages: 2,
      names: 1,
      authorities: 1,
      profiles: 1,
      rejected: 4
    })
    await writeFile(
      testInfo.outputPath('legacy-snapshot-result.json'),
      JSON.stringify(result, null, 2)
    )
  } finally {
    await writeFile(join(dir, 'stop'), '')
    await browser?.close()
    if (gateway.exitCode === null)
      await new Promise<void>((done) => {
        const timer = setTimeout(() => gateway.kill('SIGTERM'), 5000)
        gateway.once('exit', () => {
          clearTimeout(timer)
          done()
        })
      })
    await writeFile(testInfo.outputPath('legacy-processes.log'), logs.join(''))
    await rm(dir, { recursive: true, force: true })
  }
})
