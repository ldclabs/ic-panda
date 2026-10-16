import { bridge, call, code } from '../../rpc'

/**
 * Files on their way into the vault, one at a time: encrypted into the
 * vault (the import itself), every chunk and the SHA-256 checked again, then
 * held by the relay. An import is done only once the relay holds it.
 */
export type Stage = 'encrypt' | 'verify' | 'sync' | 'done'

export interface Job {
  key: number
  name: string
  stage: Stage
  /** The vault item, once encrypted. */
  id?: string
  /** The error code that stopped it. */
  error?: string
  /** The relay could not be reached; the next sync takes it. */
  waiting?: boolean
}

export const jobs = $state<Job[]>([])

let last = 0
let queue = Promise.resolve()
/** Starts the job of a dialog import once the main process names the picked file. */
let onPicked: ((name: string) => void) | null = null

function start(name: string): Job {
  jobs.push({ key: ++last, name, stage: 'encrypt' })
  return jobs[jobs.length - 1]
}

export function dismiss(key: number) {
  const index = jobs.findIndex((job) => job.key === key)
  if (index >= 0) jobs.splice(index, 1)
}

function finish(job: Job) {
  job.stage = 'done'
  setTimeout(() => dismiss(job.key), 5000)
}

/** The `file/importing` event: the person picked this file. */
export function picked(name: string) {
  onPicked?.(name)
}

/**
 * Import a dropped file, or one picked in the system dialog; `reload`
 * refreshes the list after each step. Resolves with the new item's ID, or
 * null when cancelled or stopped.
 */
export function importFile(dropped: File | undefined, reload: () => Promise<void>): Promise<string | null> {
  const run = queue.then(() => step(dropped, reload))
  queue = run.then(() => undefined)
  return run
}

async function step(dropped: File | undefined, reload: () => Promise<void>): Promise<string | null> {
  let job: Job | null = dropped ? start(dropped.name) : null
  onPicked = (name) => (job ??= start(name))
  try {
    const entry: any = await bridge(() => window.dmsg.importFile(dropped))
    if (!entry) return null
    job ??= start(entry.item?.file?.name ?? entry.item?.title ?? '')
    job.id = entry.id
    job.stage = 'verify'
    await reload()
    await call('file/verify', { id: entry.id })
    job.stage = 'sync'
    try {
      await call('sync/run')
    } catch {
      job.waiting = true
    }
    await reload()
    return entry.id
  } catch (error) {
    if (!job) job = start(dropped?.name ?? '')
    job.error = code(error)
    return null
  } finally {
    onPicked = null
  }
}

/** After a reload: the files the relay now holds are done. */
export function settle(entries: any[]) {
  for (const job of jobs)
    if (job.stage === 'sync' && !job.error && entries.some((entry) => entry.id === job.id && entry.synced)) finish(job)
}
