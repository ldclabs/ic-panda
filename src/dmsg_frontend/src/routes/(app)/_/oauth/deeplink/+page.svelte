<script lang="ts">
  import { t } from '$lib/i18n'
  import IconUser from '$lib/components/icons/IconUser1.svelte'
  import { page } from '$app/state'
  import { authStore } from '$lib/stores/auth'
  import { shortId } from '$lib/utils/auth'
  import { onMount } from 'svelte'

  const os = page.url.searchParams.get('os')
  const action = page.url.searchParams.get('action')
  const nextUrl = page.url.searchParams.get('next_url')
  const payload = page.url.hash.substring(1)

  let origin = $state('unknown')
  let error = $state('')
  let submitting = $state(false)

  let identity = $derived($authStore.identity)

  onMount(async () => {
    if (action != 'SignIn' || payload == '' || nextUrl == '' || os == '') {
      error = 'Necessary parameters are missing. Please check the deep link'
      return
    }

    try {
      let url = new URL(nextUrl!)
      if (
        url.protocol == 'https:' ||
        (url.protocol == 'http:' &&
          (url.hostname == 'localhost' || url.hostname == '127.0.0.1'))
      ) {
        origin = url.origin
      } else {
        error = $t(
          'Invalid next URL: {url}. Only HTTPS or http://localhost is allowed.',
          { url: String(nextUrl) }
        )
      }
    } catch (_) {
      error = $t('Invalid next URL: {url}', { url: String(nextUrl) })
    }
  })

  async function handleDeepLinkSignIn() {
    if (!submitting) {
      submitting = true
      const res = await authStore.deepLinkSignIn(payload)

      let url = new URL(nextUrl!)
      url.searchParams.set('os', os!)
      url.searchParams.set('action', action!)
      url.hash = res
      window.location.assign(url)
    }
  }
</script>

<svelte:head><title>{`${$t('Choose identity')} | dMsg`}</title></svelte:head>

<div class="utility-icon" aria-hidden="true"><IconUser /></div>
<h1>{$t('Choose identity')}</h1>
{#if error}
  <p class="utility-status error" role="alert">{$t(error)}</p>
{:else}
  <p class="utility-lead">{$t('to connect to')}</p>
  <p class="utility-domain"><bdi>{origin}</bdi></p>
  {#if identity.isAuthenticated}
    <button
      type="button"
      class="button primary utility-action"
      disabled={submitting}
      onclick={handleDeepLinkSignIn}
    >
      <bdi>{shortId(identity.getPrincipal().toText(), true)}</bdi>
      {#if identity.username}<span>({identity.username})</span>{/if}
    </button>
    {#if Date.now() >= identity.expiration - 1000 * 3600 * 48}
      <p class="utility-status warning"
        >{$t(
          'Your identity will expire in less than 48 hours. Please sign in again.'
        )}</p
      >
    {/if}
  {/if}
  {#if !identity.isAuthenticated || Date.now() >= identity.expiration - 1000 * 3600 * 24}
    <button
      type="button"
      class="button primary utility-action"
      disabled={submitting}
      onclick={() => authStore.signIn2('https://id.ai/authorize')}
      >Internet Identity</button
    >
  {/if}
{/if}
