import type { Action } from 'svelte/action'

/**
 * Copies `value` to the clipboard when the node is clicked, and dispatches a
 * `copyComplete` event once it lands. Replaces Skeleton's `clipboard` action;
 * only the plain-string form was ever used here.
 */
export const clipboard: Action<HTMLElement, string> = (node, value) => {
  let current = value

  const onClick = async () => {
    try {
      await copyToClipboard(current)
      node.dispatchEvent(new CustomEvent('copyComplete'))
    } catch (err) {
      console.error('Clipboard action failed:', err)
    }
  }

  node.addEventListener('click', onClick)

  return {
    update(next: string) {
      current = next
    },
    destroy() {
      node.removeEventListener('click', onClick)
    }
  }
}

async function copyToClipboard(data: string): Promise<void> {
  const clip = globalThis.navigator?.clipboard as Clipboard | undefined
  if (clip && typeof clip.writeText === 'function') {
    return clip.writeText(data)
  }

  // Fallback for insecure contexts and older browsers, where the async
  // Clipboard API is unavailable.
  const input = document.createElement('textarea')
  input.value = data
  input.setAttribute('readonly', '')
  input.style.position = 'absolute'
  input.style.left = '-9999px'
  document.body.appendChild(input)
  input.select()
  document.execCommand('copy')
  document.body.removeChild(input)
}
