import {
  ProfileError,
  type ProfilePage,
  type ProfileReadOptions,
  type ProfileWorkerRequest,
  type ProfileWorkerResponse
} from './public-profile-types'

export { ProfileError, type ProfilePage } from './public-profile-types'

// Keep cryptography, Candid decoding and their module initialization off the UI
// thread. One worker preserves the reader's cache and rollback checks on navigation.
let worker: Worker | undefined
let nextId = 0
// A worker restart must not forget a document already displayed by this page.
const seen = new Map<string, { updatedAt: number; document: string }>()
const pending = new Map<
  number,
  {
    resolve: (result: ProfilePage) => void
    reject: (error: ProfileError) => void
    clean: () => void
  }
>()

function getWorker(): Worker {
  if (worker) return worker
  const created = new Worker(
    new URL('./public-profile.worker.ts', import.meta.url),
    {
      type: 'module',
      name: 'dmsg-public-profile'
    }
  )
  created.onmessage = ({ data }: MessageEvent<ProfileWorkerResponse>) => {
    const request = pending.get(data.id)
    if (!request) return
    pending.delete(data.id)
    request.clean()
    if ('error' in data) request.reject(new ProfileError(data.error))
    else {
      const { account, document } = data.result
      const known = seen.get(account)
      const encoded = JSON.stringify(document)
      if (
        known &&
        (!document ||
          document.updated_at < known.updatedAt ||
          (document.updated_at === known.updatedAt &&
            encoded !== known.document))
      ) {
        request.reject(new ProfileError('verification_failed'))
        return
      }
      if (document)
        seen.set(account, { updatedAt: document.updated_at, document: encoded })
      request.resolve(data.result)
    }
  }
  const failed = () => {
    if (worker !== created) return
    worker = undefined
    created.terminate()
    for (const request of pending.values()) {
      request.clean()
      request.reject(new ProfileError('unavailable'))
    }
    pending.clear()
  }
  created.onerror = failed
  created.onmessageerror = failed
  worker = created
  return created
}

/** Read and verify in the background; only verified documents reach the view. */
export function readProfile(
  input: string,
  abort: AbortSignal,
  options: ProfileReadOptions = {}
): Promise<ProfilePage> {
  if (abort.aborted) return Promise.reject(new ProfileError('unavailable'))
  return new Promise((resolve, reject) => {
    let target: Worker
    try {
      target = getWorker()
    } catch {
      reject(new ProfileError('unavailable'))
      return
    }
    const id = ++nextId
    const cancel = () => {
      const request = pending.get(id)
      if (!request) return
      pending.delete(id)
      request.clean()
      reject(new ProfileError('unavailable'))
      try {
        target.postMessage({
          type: 'cancel',
          id
        } satisfies ProfileWorkerRequest)
      } catch {
        /* The worker may already have stopped. */
      }
    }
    // Bound worker startup as well as the reader's own 20-second network deadline.
    const timeout = setTimeout(cancel, 25_000)
    const clean = () => {
      clearTimeout(timeout)
      abort.removeEventListener('abort', cancel)
    }
    pending.set(id, { resolve, reject, clean })
    abort.addEventListener('abort', cancel, { once: true })
    try {
      target.postMessage({
        type: 'read',
        id,
        input,
        options
      } satisfies ProfileWorkerRequest)
    } catch {
      pending.delete(id)
      clean()
      reject(new ProfileError('unavailable'))
    }
  })
}
