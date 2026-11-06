import { daemonBlocked } from './connection-gate'
/** Only mounted, visible business views fetch message bodies. Hints carry no content. */
export function watchContent(kind: 'channel' | 'inbox', resource: () => string | null, refresh: () => unknown) {
  const run = () => {
    if (document.visibilityState === 'visible' && !daemonBlocked() && resource()) void refresh()
  }
  const off = window.dmsg.onEvent((event) => {
    if (event.type === 'resync') run()
    if (event.type !== `${kind}/hint`) return
    const id = (event.data as { id?: string } | null)?.id
    if (kind === 'inbox' || !id || id === '*' || id === resource()) run()
  })
  const timer = setInterval(run, 30_000)
  document.addEventListener('visibilitychange', run)
  return () => { off(); clearInterval(timer); document.removeEventListener('visibilitychange', run) }
}
