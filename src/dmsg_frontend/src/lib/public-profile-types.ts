import type { PrincipalDocument } from 'dmsg-sdk/principal'

export class ProfileError extends Error {
  constructor(
    readonly reason:
      | 'invalid_target'
      | 'not_found'
      | 'legacy_reserved'
      | 'unavailable'
      | 'verification_failed'
  ) {
    super(reason)
  }
}

export interface ProfilePage {
  account: string
  handle?: string | undefined
  document: PrincipalDocument | null
}

export interface ProfileReadOptions {
  refresh?: boolean
}

export type ProfileWorkerRequest =
  | { type: 'read'; id: number; input: string; options: ProfileReadOptions }
  | { type: 'cancel'; id: number }

export type ProfileWorkerResponse =
  | { id: number; result: ProfilePage }
  | { id: number; error: ProfileError['reason'] }
