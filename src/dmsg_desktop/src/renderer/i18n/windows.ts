import type { Label } from './messages/en'

/**
 * Labels that name a Mac's Touch ID or Secure Enclave, or copying from
 * another Mac, and the text that takes their place on Windows, where
 * Windows Hello and the TPM keep this device's keys.
 */
export const windowsLabels: Partial<Record<Label, Label>> = {
  touchIdUnlock: 'helloUnlock',
  lockedPresence: 'lockedHello',
  presenceBiometrics: 'presenceHello',
  afterPresence: 'afterHello',
  reasonPresenceDeclined: 'reasonHelloDeclined',
  reasonPresenceRequired: 'reasonHelloRequired',
  reasonPresenceUnavailable: 'reasonHelloUnavailable',
  deviceBoundHint: 'deviceBoundHintPc',
  deviceBoundOn: 'deviceBoundOnPc',
  deviceBoundUncovered: 'deviceBoundUncoveredPc',
  movedHint: 'movedHintPc',
  movedTitle: 'movedTitlePc',
  movedBody: 'movedBodyPc',
  movedSetAside: 'movedSetAsidePc',
  movedSetAsideHint: 'movedSetAsideHintPc',
  movedCopies: 'movedCopiesPc',
  movedCopiesHint: 'movedCopiesHintPc',
  movedReplace: 'movedReplacePc',
  movedReplaceHint: 'movedReplaceHintPc'
}
