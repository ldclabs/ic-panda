import { createProfileReader } from './public-profile-reader'
import {
  ProfileError,
  type ProfileWorkerRequest,
  type ProfileWorkerResponse
} from './public-profile-types'

const read = createProfileReader()
const active = new Map<number, AbortController>()

self.onmessage = ({ data }: MessageEvent<ProfileWorkerRequest>) => {
  if (data.type === 'cancel') {
    active.get(data.id)?.abort()
    active.delete(data.id)
    return
  }
  const controller = new AbortController()
  active.set(data.id, controller)
  void read(data.input, controller.signal, data.options)
    .then((result) => {
      if (!controller.signal.aborted)
        self.postMessage({
          id: data.id,
          result
        } satisfies ProfileWorkerResponse)
    })
    .catch((error) => {
      if (!controller.signal.aborted)
        self.postMessage({
          id: data.id,
          error: error instanceof ProfileError ? error.reason : 'unavailable'
        } satisfies ProfileWorkerResponse)
    })
    .finally(() => active.delete(data.id))
}
