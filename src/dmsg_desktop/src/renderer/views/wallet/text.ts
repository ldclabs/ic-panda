import { hasLabel, t } from '../../i18n/index.svelte'
import { deviceCapabilityName } from '../security'

export function w(key: string): string {
  if (key === 'WALLET_APPROVE_REQUIRED') {
    return t('wallet.WALLET_APPROVE_REQUIRED', { capability: deviceCapabilityName('WalletApprove', t) })
  }
  const label = `wallet.${key}`
  return hasLabel(label) ? t(label) : key
}
