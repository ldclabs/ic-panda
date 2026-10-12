// Put the dmsg binary the package bundles in resources/runtime: the release
// binary from DMSG_DESKTOP_RUNTIME (required in CI), else a local release
// build. Its version must be the app's: one release carries both.
import { execFileSync } from 'node:child_process'
import { chmod, copyFile, mkdir, readFile, rm } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Electron 44 downloads its runtime on first import; electron-builder packages
// that download (electronDist).
await import('electron')

const desktop = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const repository = resolve(desktop, '../..')
const executable = process.platform === 'win32' ? 'dmsg.exe' : 'dmsg'
let source = process.env.DMSG_DESKTOP_RUNTIME
if (process.env.CI && !source) throw new Error('Set DMSG_DESKTOP_RUNTIME to the release dmsg binary')
if (!source) {
  execFileSync('cargo', ['build', '--release', '--locked', '-p', 'dmsg_cli'], { cwd: repository, stdio: 'inherit' })
  source = resolve(repository, 'target/release', executable)
}
const { version } = JSON.parse(await readFile(resolve(desktop, 'package.json'), 'utf8'))
const reported = execFileSync(source, ['--version'], { encoding: 'utf8' }).trim().split(/\s+/).pop()
if (reported !== version) throw new Error(`${source} is dmsg ${reported}, the app is ${version}`)
const target = resolve(desktop, 'resources/runtime')
await rm(target, { recursive: true, force: true })
await mkdir(target, { recursive: true })
await copyFile(source, resolve(target, executable))
await chmod(resolve(target, executable), 0o755)
console.log(`Bundled dmsg ${reported} from ${source}`)
