import {
  idlFactory,
  type Link,
  type ProfileInfo,
  type UpdateProfileInput,
  type UploadImageInput,
  type UploadImageOutput,
  type _SERVICE
} from '$declarations/ic_message_profile/ic_message_profile.did.js'
import { agent } from '$lib/stores/auth'
import { unwrapResult } from '$lib/types/result'
import { Principal } from '@dfinity/principal'
import { readonly, writable, type Readable } from 'svelte/store'
import { createActor } from './actors'

export {
  type ProfileInfo,
  type UpdateProfileInput
} from '$declarations/ic_message_profile/ic_message_profile.did.js'

export class ProfileAPI {
  readonly canisterId: Principal
  private actor: _SERVICE
  private $myProfile: ProfileInfo | null = null
  private _myProfile = writable<ProfileInfo | null>(null)

  constructor(canister: Principal) {
    this.canisterId = canister
    this.actor = createActor<_SERVICE>({
      canisterId: canister,
      idlFactory: idlFactory
    })
  }

  get myProfileStore(): Readable<ProfileInfo | null> {
    return readonly(this._myProfile)
  }

  get myProfile(): ProfileInfo | null {
    return this.$myProfile
  }

  async refreshMyProfile(): Promise<void> {
    if (agent.isAnonymous()) {
      return
    }

    try {
      const info = await this.actor.get_profile([])
      this.$myProfile = unwrapResult(info, 'call get_profile failed')
      this._myProfile.set(this.$myProfile)
    } catch (e) {}
  }

  async get_profile(id: Principal): Promise<ProfileInfo> {
    const res = await this.actor.get_profile([id])
    return unwrapResult(res, 'call get_profile failed')
  }

  async update_profile(input: UpdateProfileInput): Promise<ProfileInfo> {
    const res = await this.actor.update_profile(input)
    return unwrapResult(res, 'call update_profile failed')
  }

  async update_profile_ecdh_pub(input: Uint8Array): Promise<null> {
    const res = await this.actor.update_profile_ecdh_pub(input)
    return unwrapResult(res, 'call update_profile_ecdh_pub failed')
  }

  async upload_image_token(
    input: UploadImageInput
  ): Promise<UploadImageOutput> {
    const res = await this.actor.upload_image_token(input)
    return unwrapResult(res, 'call upload_image_token failed')
  }

  async update_links(input: Link[]): Promise<null> {
    const res = await this.actor.update_links(input)
    return unwrapResult(res, 'call update_links failed')
  }
}
