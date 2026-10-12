/**
 * `/v1/`: the bridge window an application opens (connect-v1 §4). It
 * answers only `window.opener`, at the origin the browser reports, and
 * shows what is happening; approval always happens in dMsg.
 */
import { xidText } from '../src/bridge.ts'
import { BridgePage, type Target } from '../src/page.ts'
import { browserLabel, el, isSafari, openBridge, supported, t } from './shared.ts'

const SAFARI_IDLE_MS = 5 * 86_400_000
const CREATE = ['authenticate', 'signDocument', 'signAction', 'checkout']

const main = document.querySelector('main')!
const status = el('p', { class: 'status', 'data-testid': 'status' })
const activity = el('section', { class: 'activity', 'data-testid': 'activity' })
const prompt = el('section', { class: 'prompt' })
main.append(status, prompt, activity)
document.title = t('title')

/** The person's choice among `options`, in the window. */
function ask(question: string, options: { label: string; value: string }[]): Promise<string> {
  return new Promise((resolve) => {
    prompt.replaceChildren(
      el('p', {}, question),
      ...options.map((o) => {
        const button = el('button', { type: 'button', 'data-testid': `choose-${o.value}` }, o.label)
        button.addEventListener('click', () => {
          prompt.replaceChildren()
          resolve(o.value)
        })
        return button
      })
    )
  })
}

function note(...children: (Node | string)[]) {
  activity.prepend(el('div', { class: 'card' }, ...children))
}

const bridge = await openBridge()
const accounts = () => bridge.active().map((p) => `${xidText(p.account)} · ${p.label}`)
const ok = await supported()
if (!ok) status.replaceChildren(el('span', { class: 'error', 'data-testid': 'unsupported' }, t('unsupported')))
else if (!window.opener) {
  const list = accounts()
  status.replaceChildren(t('noOpener'))
  activity.replaceChildren(
    el('h2', {}, t('paired')),
    list.length ? el('ul', {}, ...list.map((a) => el('li', { class: 'mono' }, a))) : el('p', { class: 'muted' }, t('none'))
  )
} else status.textContent = '…'

// Safari may drop this site's storage after 7 days without a click here (§4.1).
document.addEventListener('click', () => void bridge.store.setMeta('lastClick', Date.now()))

bridge.chooseAccount = (list) => ask(t('choose'), list.map((a) => ({ label: a, value: a })))

const page = new BridgePage(bridge, {
  opener: () => (window.opener as Target | null) ?? null,
  connected: (c) => {
    status.replaceChildren(
      el('strong', {}, t('connected')),
      ' ',
      el('span', { class: 'mono', 'data-testid': 'origin' }, c.origin),
      ` · ${c.appId}`
    )
    status.dataset.state = 'connected'
  },
  refused: (origin, code) => {
    status.replaceChildren(el('span', { class: 'error' }, `${t('failed')}: `), el('span', { class: 'mono' }, origin), ` · ${code}`)
    status.dataset.state = 'refused'
  },
  ended: () => {
    status.textContent = t('ended')
    status.dataset.state = 'ended'
  },
  done: (method, outcome) => {
    if (outcome.error === 'NOT_PAIRED') {
      const href = outcome.account ? `dmsg://connect/pair?account=${outcome.account}` : 'dmsg://connect/pair'
      note(el('p', {}, t('notPaired')), el('a', { class: 'button', href, 'data-testid': 'pair-link' }, t('openDmsg')))
      return
    }
    if (outcome.error) {
      if (CREATE.includes(method)) note(el('p', { class: 'error' }, `${t('failed')}: ${outcome.error}`))
      return
    }
    const value = outcome.value!
    if (CREATE.includes(method) && value.state === 'awaiting_user' && outcome.link) {
      const href = `dmsg://connect/request?account=${outcome.link.account}&request=${outcome.link.requestId}`
      note(
        el('p', { 'data-testid': 'sent' }, t('sent')),
        el('a', { class: 'button', href, 'data-testid': 'request-link' }, t('confirm')),
        el('p', { class: 'muted' }, t('approvals'))
      )
    } else if (method === 'getOperation' && value.state === 'signed') {
      note(el('p', { 'data-testid': 'signed' }, t('signed')))
    }
  },
  beforeCreate: async () => {
    if (!isSafari) return
    const last = (await bridge.store.meta('lastClick')) as number | undefined
    if (last && Date.now() - last < SAFARI_IDLE_MS) return
    await ask(t('safari'), [{ label: t('continue'), value: 'continue' }])
  }
})
page.supported = ok
window.addEventListener('message', (event) => void page.receive(event))
setInterval(() => page.tick(), 1000)
document.documentElement.dataset.label = browserLabel()
document.documentElement.dataset.ready = 'true'
