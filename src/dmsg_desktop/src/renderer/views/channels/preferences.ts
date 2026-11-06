/** Channel choices kept on this device: read receipts for all accounts, pins per account. */
function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* kept until the window closes */
  }
}

export const readReceipts = () => read('dmsg.read-receipts') === 'on'
export const setReadReceipts = (on: boolean) => write('dmsg.read-receipts', on ? 'on' : 'off')

export const pinsOf = (account: string | undefined): string[] => (account ? (read(`dmsg.channel-pins.${account}`) ?? '').split(',').filter(Boolean) : [])
export const setPins = (account: string, pins: string[]) => write(`dmsg.channel-pins.${account}`, pins.join(','))
