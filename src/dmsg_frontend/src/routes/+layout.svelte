<script lang="ts">
  import { onMount } from 'svelte'
  import { initLocale, t } from '$lib/i18n'
  import '$lib/i18n/locale.css'
  let ready = false
  onMount(async () => {
    await initLocale()
    ready = true
  })
</script>

{#if ready}<slot />{:else}<div
    class="locale-loading"
    role="status"
    aria-label={$t('Loading…')}
  ></div>{/if}

<style>
  .locale-loading {
    width: 24px;
    height: 24px;
    margin: 25vh auto;
    border: 2px solid #aaa;
    border-top-color: #145c45;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .locale-loading {
      animation: none;
    }
  }
</style>
