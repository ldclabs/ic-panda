import type { IcrcAccount } from '@dfinity/oisy-wallet-signer'
import { IcrcWallet } from '@dfinity/oisy-wallet-signer/icrc-wallet'
import type { Principal } from '@dfinity/principal'

const OISY_WALLET_HOST = 'https://oisy.com/sign'

function initOISYWallet() {
  let wallet: IcrcWallet | null = null
  let account: IcrcAccount | null = null

  return {
    account: () => account,
    wallet: () => wallet,
    onclose: async () => {
      await wallet?.disconnect()
      wallet = null
      account = null
    },

    connect: async (): Promise<IcrcAccount> => {
      if (account) {
        return account
      }

      wallet = await IcrcWallet.connect({
        url: OISY_WALLET_HOST
        // host: 'http://localhost:4943'
      })
      const { allPermissionsGranted } =
        await wallet.requestPermissionsNotGranted()
      if (!allPermissionsGranted) {
        await wallet.disconnect()
        wallet = null
        throw new Error('Permissions not granted')
      }

      const accounts = await wallet.accounts()
      await wallet.disconnect()
      account = accounts?.[0] || null
      if (!account) {
        wallet = null
        throw new Error('No account found')
      }
      return account
    },

    transfer: async (
      to: Principal,
      ledgerId: string,
      amount: bigint
    ): Promise<bigint> => {
      if (!account) {
        throw new Error('Wallet not connected')
      }

      wallet = await IcrcWallet.connect({
        url: OISY_WALLET_HOST
        // host: 'http://localhost:4943'
      })

      try {
        return await wallet.transfer({
          params: {
            to: { owner: to, subaccount: [] },
            amount,
            memo: new Uint8Array(0)
          },
          owner: account.owner.toString(),
          ledgerCanisterId: ledgerId
        })
      } catch (err) {
        throw err
      } finally {
        await wallet.disconnect()
      }
    }
  }
}

export const oisyWallet = initOISYWallet()
