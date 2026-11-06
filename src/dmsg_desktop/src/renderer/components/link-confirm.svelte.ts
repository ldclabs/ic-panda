import { bridge } from '../rpc'
import { errorText } from '../rpc'
import { toast } from './toast.svelte'

/** The web address waiting for the person to confirm where it goes. */
export const leaving = $state<{ url: string; host: string } | { url: null; host: null }>({ url: null, host: null })

/** A `dmsg://` link only moves a view; a web address says where it goes first. */
export function followLink(url: string, host: string) {
  if (/^dmsg:/i.test(url)) {
    void bridge(() => window.dmsg.openLink(url)).catch((error) => toast(() => errorText(error)))
    return
  }
  leaving.url = url
  leaving.host = host
}
