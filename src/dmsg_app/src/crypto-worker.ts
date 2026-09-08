import { CryptoEngine } from './lib/crypto/engine'
import { errorText } from './lib/errors'

const scope = self as unknown as DedicatedWorkerGlobalScope
let generation = ''
let engine: CryptoEngine
let queue = Promise.resolve()
const allowed = new Set([
  'status',
  'initialize',
  'unlock',
  'tick',
  'lock',
  'pendingRecovery',
  'verifyRecovery',
  'view',
  'saveItem',
  'deleteItem',
  'resolveConflict',
  'createChannel',
  'saveMessage',
  'saveDraft',
  'getDraft',
  'saveProfile',
  'importFile',
  'downloadFile',
  'changePassword',
  'exportBackup',
  'restore',
  'readRequest',
  'authSign'
])
scope.onmessage = (event) => {
  const request = event.data
  if (
    !request ||
    typeof request.generation !== 'string' ||
    typeof request.id !== 'number' ||
    !allowed.has(request.method)
  )
    return
  if (!generation) {
    generation = request.generation
    engine = new CryptoEngine(
      (progress) => scope.postMessage({ type: 'progress', generation, progress }),
      generation
    )
  }
  if (generation !== request.generation) return
  // Serial execution prevents an asynchronous unlock/import from completing
  // over a newer operation. Locking terminates this worker at the page boundary.
  queue = queue.then(async () => {
    try {
      engine.setDocumentId(request.documentId)
      const fn = engine[request.method as keyof CryptoEngine] as (
        ...args: unknown[]
      ) => Promise<unknown>
      const value = await fn.apply(engine, request.args ?? [])
      scope.postMessage({ id: request.id, generation, ok: true, value })
    } catch (error) {
      scope.postMessage({ id: request.id, generation, ok: false, error: errorText(error) })
    }
  })
}
