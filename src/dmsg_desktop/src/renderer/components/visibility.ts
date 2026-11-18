import type { Attachment } from 'svelte/attachments'

const listeners = new WeakMap<Element, (visible: boolean) => void>()
let observer: IntersectionObserver | undefined

/** Share one viewport observer across long lists of account names and avatars. */
export function visibility(setVisible: (visible: boolean) => void): Attachment {
  return node => {
    observer ??= new IntersectionObserver(entries => {
      for (const entry of entries) listeners.get(entry.target)?.(entry.isIntersecting)
    })
    listeners.set(node, setVisible)
    observer.observe(node)
    return () => { observer!.unobserve(node); listeners.delete(node) }
  }
}
