<script lang="ts">
  import { page } from '$app/state'
  import PublicProfile from '$lib/components/profile/PublicProfile.svelte'
</script>

<PublicProfile identifier={page.params['username'] || ''} />
