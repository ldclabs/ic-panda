<script lang="ts">
  import { goto } from '$app/navigation'
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
        error = `Invalid next URL: "${nextUrl}"", only "https://" or "http://localhost" is allowed`
      }
    } catch (_) {
      error = `Invalid next URL: "${nextUrl}"`
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

<div class="flex flex-col items-center justify-center p-4">
  <div
    class="w-full max-w-2xl space-y-8 rounded-lg bg-white p-6 shadow-md dark:bg-neutral-900"
  >
    <div class="text-center">
      <h1 class="text-gray-900 text-2xl font-normal dark:text-white"
        >Choose Identity 🔑</h1
      >
      <p class="text-gray-600 dark:text-gray-400 mt-2">
        to connect to <span class="text-pretty break-words text-2xl font-bold"
          >{origin}</span
        >
      </p>
    </div>

    {#if identity.isAuthenticated}
      <div class="mt-8 flex flex-col items-center">
        <button
          type="button"
          class="variant-filled-primary btn w-96 max-w-full"
          disabled={submitting || error != ''}
          onclick={handleDeepLinkSignIn}
        >
          <span>{shortId(identity.getPrincipal().toText(), true)}</span>
          {#if identity.username}
            <span class="truncate">
              ({identity.username})
            </span>
          {/if}
        </button>
        {#if Date.now() >= identity.expiration - 1000 * 3600 * 48}
          <p class="mt-2 text-sm text-error-500">
            Your identity will expire in less than 48 hours. Please sign in
            again.
          </p>
        {/if}
      </div>
    {/if}
    {#if !identity.isAuthenticated || Date.now() >= identity.expiration - 1000 * 3600 * 24}
      <div class="mt-8 flex flex-col items-center space-y-4">
        <button
          type="button"
          class="variant-filled-primary btn w-80"
          disabled={submitting}
          onclick={() => {
            authStore.signIn2()
          }}
        >
          Sign in with Internet Identity
        </button>
        <button
          type="button"
          class="variant-filled-secondary btn w-80"
          disabled={submitting}
          onclick={() => {
            authStore.signIn()
          }}
        >
          Sign in with identity.ic0.app (legacy)
        </button>
      </div>
    {/if}
    {#if error}
      <div class="mt-8 flex flex-col items-center space-y-4">
        <p class="mt-2 text-lg text-error-500">{error}</p>
      </div>
    {/if}

    <div class="mt-6 text-center">
      <button class="text-sm" onclick={() => goto('/')}>
        Return to the <span class="font-bold">dMsg.net</span> home page
      </button>
    </div>
  </div>
</div>
