<script lang="ts">
  import { page } from '$app/stores'
  import Loading from '$lib/components/ui/Loading.svelte'

  const username = ($page?.params || {})['username'] || ''
  window.location.assign(`https://dmsg.net/${username}`)
</script>

<div class="flex h-full w-full flex-col items-center justify-center">
  <Loading />
</div>
