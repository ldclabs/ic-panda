import { nativeLink } from '../src/native-link.ts'
import { el, t } from './shared.ts'
const main = document.querySelector('main')!
document.title = t('openDmsg')
try {
  const url = new URL(location.href), kind = url.pathname.replace(/\/$/, '').split('/').at(-1)
  if (kind !== 'pair' && kind !== 'request') throw new Error('Invalid link')
  const keys = [...url.searchParams.keys()]
  if (new Set(keys).size !== keys.length || keys.some(k => !['account', 'request'].includes(k)) || url.hash) throw new Error('Invalid link')
  const href = nativeLink(kind, url.searchParams.get('account') ?? undefined, url.searchParams.get('request') ?? undefined, false)
  main.append(
    el('h1', {}, t('openDmsg')),
    el('p', {}, kind === 'pair' ? t('pairInApp') : t('approvals')),
    el('div', { class: 'actions' },
      el('a', { class: 'button', href }, t('openDmsg')),
      el('a', { class: 'button secondary', href: 'https://apps.apple.com/app/id6827050680' }, 'App Store')
    )
  )
} catch { main.append(el('h1', {}, t('openDmsg')), el('p', { class: 'error', role: 'alert' }, t('invalid'))) }
