<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { onMount } from 'svelte'

  $: username = ($page?.params || {})['username'] || ''
  onMount(async () => {
    goto(`https://dmsg.net/${username}`)
  })
</script>

<Loading />
