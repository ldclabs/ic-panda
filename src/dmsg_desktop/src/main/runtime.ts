/**
 * The `dmsg` the desktop runs. A packaged app carries its release's `dmsg`
 * and runs it only as `dmsg install`, which puts it at the shared location
 * unless the one there is newer or Homebrew's and, on macOS, signed by the
 * same Developer ID team; the app then runs the
 * installed one, so the CLI, agents and the app share one binary and one
 * daemon. Development builds use `DMSG_BIN` or `dmsg` on PATH.
 */
import { execFile } from 'node:child_process'
import { join } from 'node:path'
import { promisify } from 'node:util'

const run = promisify(execFile)

export interface Runtime {
  /** The binary to run. */
  binary: string
  /** What `dmsg install` did: installed, upgraded, replaced, current, kept_newer or homebrew. */
  action?: string
  /** The installed binary's version, when known. */
  version?: string
}

export async function resolveRuntime(packaged: boolean, resources: string): Promise<Runtime> {
  if (!packaged) return { binary: process.env.DMSG_BIN || 'dmsg' }
  const bundled = join(resources, 'runtime', process.platform === 'win32' ? 'dmsg.exe' : 'dmsg')
  const { stdout } = await run(bundled, ['--json', 'install'], { timeout: 60_000 })
  const report = JSON.parse(stdout.trim().split('\n').pop()!) as { action: string; path: string; version?: string }
  return { binary: report.path, action: report.action, version: report.version }
}
