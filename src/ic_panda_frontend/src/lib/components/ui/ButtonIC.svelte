<script lang="ts">
  import { t } from '$lib/i18n'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
</script>

<button type="button" class="variant-filled-secondary btn max-md:px-3" on:click>
  <span class="max-md:hidden"><IconIcLogo /></span>
  <span class="max-md:!ml-0">{$t('Sign In')}</span>
</button>
