import { onSettled, runningSince, startedCalls } from '../rpc'

/**
 * A button its click disabled shows a turning ring while the bridge calls
 * that click started run, so a wait does not read as a hang. Views keep
 * disabling buttons by their own `busy` flags; this follows only the button
 * clicked, and lets go once it is enabled again, gone, or its calls ended.
 * A wait shorter than `DELAY_MS` shows nothing.
 */
const DELAY_MS = 150

const watched = new Map<HTMLButtonElement, { mark: number; observer: MutationObserver }>()

function update(button: HTMLButtonElement) {
  const entry = watched.get(button)
  if (!entry || (button.isConnected && button.disabled && runningSince(entry.mark))) return
  entry.observer.disconnect()
  watched.delete(button)
  delete button.dataset.pending
}

export function showPendingButtons() {
  // Capture: the count is taken before the click's own handlers start their calls.
  document.addEventListener(
    'click',
    (event) => {
      const button = event.target instanceof Element ? event.target.closest('button') : null
      if (!button || button.disabled) return
      const mark = startedCalls()
      setTimeout(() => {
        if (watched.has(button) || !button.isConnected || !button.disabled || !runningSince(mark)) return
        const observer = new MutationObserver(() => update(button))
        observer.observe(button, { attributes: true, attributeFilter: ['disabled'] })
        watched.set(button, { mark, observer })
        // The ring is placed over the label: a static button becomes its containing block.
        button.dataset.pending = getComputedStyle(button).position === 'static' ? 'static' : ''
      }, DELAY_MS)
    },
    true
  )
  // After the calls a finished one's continuation starts, as a sequence of awaits does.
  onSettled(() => setTimeout(() => watched.forEach((_, button) => update(button))))
}
