<script lang="ts">
  import type { Snippet } from 'svelte'
  import Brand from '$lib/components/landing/Brand.svelte'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import '$lib/components/landing/site.css'
  import './utility.css'

  let { children }: { children: Snippet } = $props()
</script>

<div class="dmsg-site utility-site">
  <a class="skip" href="#main">{$t('Skip to content')}</a>
  <header class="site-header">
    <div class="wrap nav-wrap"><Brand /><LocaleSwitcher /></div>
  </header>
  <main id="main" class="utility-main" tabindex="-1">
    <section class="utility-card">{@render children()}</section>
    <a class="text-link utility-back" href="/" data-sveltekit-reload
      >{$t('Back to dMsg')}</a
    >
  </main>
</div>
