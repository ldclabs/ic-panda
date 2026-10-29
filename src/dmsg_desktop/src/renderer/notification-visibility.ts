import { onMount } from 'svelte'
import type { NoticeVisibility } from '../shared/contract'

/** Report loaded business data only when its anchor is in the focused viewport.
 * No business mark/read/ack is performed here. A dialog or page departure clears
 * the native context; asynchronous reads must still use each view's own fence. */
export function noticeVisibility(read: () => NoticeVisibility | null, selector: string): () => void {
  let alive = false, frame = 0, intersecting = false, last = ''
  let element: Element | null = null
  let observer: IntersectionObserver
  const clear = () => {
    if (!last) return
    last = ''
    void window.dmsg.notificationVisible(null).catch(() => {})
  }
  const report = () => {
    frame = 0
    if (!alive) return
    const value = read()
    const next = value ? document.querySelector(selector) : null
    if (next !== element) {
      observer.disconnect()
      element = next
      intersecting = false
      if (element) observer.observe(element)
    }
    if (!value || !intersecting || document.body.inert || !document.hasFocus() || document.visibilityState !== 'visible' || document.querySelector('[aria-modal="true"]')) { clear(); return }
    const key = JSON.stringify(value)
    if (key === last) return
    last = key
    void window.dmsg.notificationVisible(value).then((accepted) => {
      if (!accepted && last === key) last = ''
    }, () => { if (last === key) last = '' })
  }
  const refresh = () => { if (alive && !frame) frame = requestAnimationFrame(report) }
  onMount(() => {
    alive = true
    observer = new IntersectionObserver((entries) => {
      const entry = entries.find((entry) => entry.target === element)
      if (entry) intersecting = entry.isIntersecting && entry.intersectionRatio >= 0.1
      refresh()
    }, { threshold: [0, 0.1] })
    // Handles portal dialogs and view DOM replacement without a polling loop.
    const changes = new MutationObserver(refresh)
    changes.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['aria-modal', 'aria-hidden', 'inert'] })
    window.addEventListener('focus', refresh)
    window.addEventListener('blur', clear)
    document.addEventListener('visibilitychange', refresh)
    const off = window.dmsg.onEvent((event) => {
      if (['resync', 'session/changed', 'connection/changed', 'notifications/focus'].includes(event.type)) { last = ''; refresh() }
    })
    refresh()
    return () => {
      alive = false
      cancelAnimationFrame(frame)
      observer.disconnect()
      changes.disconnect()
      window.removeEventListener('focus', refresh)
      window.removeEventListener('blur', clear)
      document.removeEventListener('visibilitychange', refresh)
      off()
      clear()
    }
  })
  return refresh
}
