/** Lock and approval badges are independent; only a confirmed lock shows the padlock. */
export function trayVariant(state: string, waiting: number): 'idle' | 'waiting' | 'locked' | 'lockedWaiting' {
  if (state === 'locked') return waiting > 0 ? 'lockedWaiting' : 'locked'
  return waiting > 0 ? 'waiting' : 'idle'
}
