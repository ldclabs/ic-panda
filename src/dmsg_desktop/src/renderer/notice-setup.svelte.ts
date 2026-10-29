/**
 * Accounts whose security-alert prompt was set aside on this device. Binding
 * another login shows it again; turning alerts on hides it by itself.
 */
const key = 'dmsg.notice-setup'
let dismissed = $state<string[]>(stored())

function stored(): string[] {
  try {
    return (localStorage.getItem(key) ?? '').split(',').filter(Boolean)
  } catch {
    return []
  }
}

function keep(next: string[]) {
  dismissed = next
  try {
    localStorage.setItem(key, next.join(','))
  } catch {
    /* the choice lasts until the window closes */
  }
}

export const setupDismissed = (account: string) => dismissed.includes(account)
export const dismissSetup = (account: string) => { if (!setupDismissed(account)) keep([...dismissed, account]) }
export const offerSetupAgain = (account: string) => { if (setupDismissed(account)) keep(dismissed.filter((id) => id !== account)) }
