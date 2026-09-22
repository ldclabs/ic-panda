import { spawn } from 'node:child_process'
if (!process.env.DMSG_CLOUD_DIR)
  throw new Error('Set DMSG_CLOUD_DIR to the private local relay checkout')
const suite = process.argv[2] === 'account' ? 'account' : 'cloud'
const child = spawn('pnpm', ['exec', 'playwright', 'test', `e2e/${suite}.spec.ts`], {
  stdio: 'inherit',
  env: process.env
})
child.once('error', (error) => {
  console.error(error.message)
  process.exitCode = 1
})
child.once('exit', (code) => {
  process.exitCode = code ?? 1
})
