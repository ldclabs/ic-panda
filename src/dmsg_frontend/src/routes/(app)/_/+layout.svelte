<script lang="ts">
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
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
    <LocaleSwitcher />
    <a href="/legacy" class="archive-label"
      >{$t('Legacy archive')} <span>{$t('Read-only')}</span></a
    >
    <nav aria-label={$t('Archive navigation')}>
      <a
        href="/_/messages"
        aria-current={!page.url.pathname.startsWith('/_/profile')
          ? 'page'
          : undefined}>{$t('Messages')}</a
      >
      <a
        href="/_/profile"
        aria-current={page.url.pathname.startsWith('/_/profile')
          ? 'page'
          : undefined}>{$t('Profile')}</a
      >
      <MoreMenuPopup triggerClass="text-link"
        >{#snippet trigger()}{$t('Account')}{/snippet}</MoreMenuPopup
      >
    </nav>
  </header>
  <div class="archive-content">
    {#key $authStore.identity
      .getPrincipal()
      .toText()}{@render children?.()}{/key}
  </div>
</div>
