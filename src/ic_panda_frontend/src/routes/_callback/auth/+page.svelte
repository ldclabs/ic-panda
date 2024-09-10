<script lang="ts">
  import { page } from '$app/stores'
  import { X_AUTH_KEY } from '$lib/constants'
  import { type AuthMessage } from '$lib/types/auth'
  import { onMount } from 'svelte'

  let msg: AuthMessage<string> | null = null
  onMount(async () => {
    const hash = $page.url.hash
    if (hash.startsWith('#challenge=')) {
      msg = {
        kind: 'XAuth',
        result: hash.slice(11)
      }
    } else if (hash.startsWith('#error=')) {
      msg = {
        kind: 'XAuth',
        error: decodeURIComponent(hash.slice(7))
      }
    } else {
      msg = {
        kind: 'XAuth',
        error: 'Invalid callback, missing challenge or error.'
      }
    }

    localStorage.setItem(X_AUTH_KEY, JSON.stringify(msg))
    setTimeout(() => {
      window.close()
    }, 1000)
  })
</script>
