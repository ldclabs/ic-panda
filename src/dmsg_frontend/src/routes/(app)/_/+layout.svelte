<script lang="ts">
  import { page } from '$app/state'
  import MoreMenuPopup from '$lib/components/core/MoreMenuPopup.svelte'
  import Brand from '$lib/components/landing/Brand.svelte'
  import { authStore } from '$lib/stores/auth'
  import type { Snippet } from 'svelte'
  import '$lib/components/landing/landing.css'
  import '$lib/components/legacy/legacy.css'

  let { children }: { children?: Snippet } = $props()
</script>

<div class="dmsg-site legacy-app">
  <header class="archive-header">
    <Brand />
    <a href="/legacy" class="archive-label"
      >Legacy archive <span>Read-only</span></a
    >
    <nav aria-label="Archive navigation">
      <a
        href="/_/messages"
        aria-current={!page.url.pathname.startsWith('/_/profile')
          ? 'page'
          : undefined}>Messages</a
      >
      <a
        href="/_/profile"
        aria-current={page.url.pathname.startsWith('/_/profile')
          ? 'page'
          : undefined}>Profile</a
      >
      <MoreMenuPopup triggerClass="text-link"
        >{#snippet trigger()}Account{/snippet}</MoreMenuPopup
      >
    </nav>
  </header>
  <div class="archive-content">
    {#key $authStore.identity
      .getPrincipal()
      .toText()}{@render children?.()}{/key}
  </div>
</div>
