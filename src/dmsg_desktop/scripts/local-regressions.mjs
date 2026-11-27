// Run the display-dependent suites serially so keyboard focus and native windows belong to one fixture.
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const env = { ...process.env }
delete env.DMSG_DESKTOP_SMOKE
delete env.DMSG_SMOKE_SCOPE
delete env.DMSG_DESKTOP_RECOVERY_ONLY
const desktop = fileURLToPath(new URL('../', import.meta.url))
function run(suite, options = {}) {
  console.log(`Running local Electron suite ${options.DMSG_DESKTOP_SMOKE ?? suite}`)
  const result = spawnSync(process.execPath, [`scripts/${suite}.mjs`], { cwd: desktop, env: { ...env, ...options }, stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}
run('smoke')
run('smoke', { DMSG_DESKTOP_SMOKE: 'startup' })
run('smoke', { DMSG_DESKTOP_SMOKE: 'startup-status' })
run('smoke', { DMSG_DESKTOP_SMOKE: 'succession' })
run('smoke', { DMSG_DESKTOP_SMOKE: 'notifications', DMSG_DESKTOP_SHOW: '1' })
for (const suite of ['channel-messages-smoke', 'approval-smoke', 'wallet-upgrade-smoke', 'vault-detail-smoke', 'references-smoke', 'secrets-smoke', 'update-install-smoke']) run(suite)
console.log('All local Electron suites passed')
