<script lang="ts">
  // dmsg-login/1 (docs/protocol/login-v1_zh.md): Internet Identity signs a
  // delegation to the local dMsg client's session key; this page keeps no
  // session and holds no key, and returns only to an allowlisted callback.
  // Internet Identity ignores ICRC-34 targets, so none are asked for here:
  // the client limits the chain to its canisters itself.
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import IconUser from '$lib/components/icons/IconUser1.svelte'
  import { t } from '$lib/i18n'
  import { complete, takeFragment } from '$lib/login/flow'
  import {
    completePasskey,
    parsePasskeyRequest,
    type PasskeyRequest
  } from '$lib/login/passkey'
  import {
    callbackTarget,
    parseRequest,
    type LoginRequest
  } from '$lib/login/protocol'
  import { Ed25519PublicKey } from '@icp-sdk/core/identity'
  import { Signer } from '@icp-sdk/signer'
  import { PostMessageTransport } from '@icp-sdk/signer/web'
  import { onMount } from 'svelte'

  let request = $state<LoginRequest | null>(null)
  let passkey = $state<PasskeyRequest | null>(null)
  let invalid = $state(false)
  let busy = $state(false)

  onMount(() => {
    try {
      const fragment = takeFragment(location, history)
      if (fragment.length > 32_768) throw new Error('size')
      try {
        request = parseRequest(fragment)
      } catch {
        passkey = parsePasskeyRequest(fragment)
      }
    } catch {
      invalid = true
    }
  })

  async function delegate(request: LoginRequest) {
    const signer = new Signer({
      transport: new PostMessageTransport({
        url: 'https://id.ai/authorize',
        windowOpenerFeatures: 'width=576,height=680'
      }),
      derivationOrigin: request.derivation_origin
    })
    await signer.openChannel()
    try {
      const chain = await signer.requestDelegation({
        publicKey: Ed25519PublicKey.fromDer(request.session_pubkey),
        maxTimeToLive: BigInt(request.ttl_ms) * 1_000_000n
      })
      return JSON.stringify(chain.toJSON())
    } finally {
      await signer.closeChannel()
    }
  }

  function start() {
    if (passkey && !busy) {
      busy = true
      void completePasskey(passkey, navigator.credentials, (url) =>
        location.assign(url)
      )
      return
    }
    if (!request || busy) return
    busy = true
    void complete(request, delegate, (url) => location.assign(url))
  }
</script>

<svelte:head>
  <title>{$t('Sign in to dMsg')}</title>
</svelte:head>

<UtilityPage>
  <div class="utility-icon" aria-hidden="true"><IconUser /></div>
  <h1>{$t('Sign in to dMsg')}</h1>
  {#if invalid}
    <p class="utility-status error" role="alert">
      {$t(
        'This login link is invalid or incomplete. Start the login again from the dMsg app.'
      )}
    </p>
  {:else if passkey}
    <p class="utility-lead"
      >{passkey.challenge.action === 'logout_all'
        ? $t('Confirm with your passkey to sign out all devices.')
        : $t('Use your passkey to continue to the dMsg app.')}</p
    >
    <button
      type="button"
      class="button primary utility-action"
      disabled={busy}
      onclick={start}
      aria-busy={busy}
    >
      {busy
        ? $t('Waiting for your passkey…')
        : passkey.challenge.action === 'register'
          ? $t('Create a passkey')
          : passkey.challenge.action === 'logout_all'
            ? $t('Sign out all devices')
            : $t('Continue with a passkey')}
    </button>
    <p class="utility-note"
      >{passkey.challenge.action === 'logout_all'
        ? $t(
            'Existing login authorizations may remain valid for up to 12 hours. Your content devices stay authorized.'
          )
        : $t(
            'Your device keeps you signed in. Unlocking your local content still requires your confirmation.'
          )}</p
    >
  {:else if request}
    <p class="utility-lead"
      >{#if callbackTarget(request.next_url) === 'ios'}
        {$t('Return to the dMsg iOS app after signing in.')}
      {:else}
        {$t('Connecting to dMsg on this computer (port {port}).', {
          port: callbackTarget(request.next_url)
        })}
      {/if}</p
    >
    <p class="utility-domain"
      >{$t('Identity domain: {origin}', {
        origin: request.derivation_origin
      })}</p
    >
    <button
      type="button"
      class="button primary utility-action"
      disabled={busy}
      onclick={start}
      aria-busy={busy}
    >
      {busy
        ? $t('Waiting for Internet Identity…')
        : $t('Continue with Internet Identity')}
    </button>
    <p class="utility-note"
      >{$t(
        'Internet Identity lets the dMsg app act for you. This page keeps no session.'
      )}</p
    >
  {:else}
    <p class="utility-status" role="status">{$t('Loading…')}</p>
  {/if}
</UtilityPage>
