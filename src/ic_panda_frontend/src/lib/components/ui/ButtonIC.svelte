<script lang="ts">
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
</script>

<button type="button" class="btn variant-filled" on:click>
  <span><IconIcLogo /></span>
  <span class="text-left">
    <span><slot name="action" /></span>
    <span><slot /></span>
  </span>
</button>
