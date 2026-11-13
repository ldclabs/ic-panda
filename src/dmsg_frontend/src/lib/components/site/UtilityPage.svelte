<script lang="ts">
  import type { Snippet } from 'svelte'
  import Brand from '$lib/components/landing/Brand.svelte'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import '$lib/components/landing/site.css'
  import './utility.css'

  // `aside` takes the place of the way back, for a page whose reader never came
  // from dMsg.
  let { children, aside }: { children: Snippet; aside?: Snippet } = $props()
</script>

<div class="dmsg-site utility-site">
  <a
    class="skip"
    href="#main"
    onclick={(event) => {
      // Share and request links keep their decryption key in the fragment.
      event.preventDefault()
      document.getElementById('main')?.focus()
    }}>{$t('Skip to content')}</a
  >
  <header class="site-header">
    <div class="wrap nav-wrap"><Brand /><LocaleSwitcher /></div>
  </header>
  <main id="main" class="utility-main" tabindex="-1">
    <section class="utility-card">{@render children()}</section>
    {#if aside}{@render aside()}{:else}<a
        class="text-link utility-back"
        href="/"
        data-sveltekit-reload>{$t('Back to dMsg')}</a
      >{/if}
  </main>
</div>
