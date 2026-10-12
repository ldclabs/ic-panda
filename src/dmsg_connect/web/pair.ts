/**
 * `/v1/pair#<invite>`: the page a device opens to connect this browser
 * (connect-v1 §5.2). The fragment never reaches a server and is cleared as
 * soon as it is read; the device shows the label and the person confirms.
 */
import { ConnectError } from '../src/errors.ts'
import { browserLabel, el, openBridge, supported, t } from './shared.ts'

const main = document.querySelector('main')!
document.title = t('pairTitle')
const fragment = location.hash.slice(1)
history.replaceState(null, '', location.pathname)

function show(state: string, ...children: (Node | string)[]) {
  main.dataset.state = state
  main.replaceChildren(el('h1', {}, t('pairTitle')), ...children)
}

if (!(await supported())) show('error', el('p', { class: 'error' }, t('unsupported')))
else if (!fragment) show('error', el('p', {}, t('pairMissing')))
else {
  const label = browserLabel()
  try {
    const bridge = await openBridge()
    const { account, pairingId } = await bridge.pair(fragment, label)
    main.dataset.account = account
    main.dataset.pairing = pairingId
    show('waiting', el('p', {}, t('pairWaiting')), el('p', { class: 'label', 'data-testid': 'label' }, label))
    await bridge.awaitGrant(account, 600_000)
    show(
      'active',
      el('p', {}, `${t('pairDone')} `, el('span', { class: 'mono', 'data-testid': 'account' }, account)),
      el('p', { class: 'muted' }, t('pairClose'))
    )
  } catch (error) {
    // A link from another environment is the only FORBIDDEN before the relay answers.
    const message = error instanceof ConnectError && error.code === 'FORBIDDEN' ? t('otherEnvironment') : t('invalid')
    show('error', el('p', { class: 'error', 'data-testid': 'error' }, message))
    main.dataset.error = error instanceof ConnectError ? error.code : 'UNAVAILABLE'
  }
}

