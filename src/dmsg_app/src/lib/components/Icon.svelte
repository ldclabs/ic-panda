<script lang="ts">
  let { name, size = 20 }: { name: string; size?: number } = $props()
  const available = new Set([
    'chrome',
    'menu',
    'cloud',
    'eye',
    'device',
    'arrow-up-right',
    'chat',
    'signature',
    'copy',
    'close',
    'check',
    'download',
    'shield',
    'arrow-right',
    'key',
    'lock',
    'file',
    'github'
  ])
</script>

{#if available.has(name)}
  <span
    class="icon"
    style:width="{size}px"
    style:height="{size}px"
    style:mask-image="url('./assets/{name}.svg')"
    aria-hidden="true"
  ></span>
{:else}
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.6"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    {#if name === 'plus'}<path d="M12 5v14M5 12h14" />
    {:else if name === 'search'}<circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 4 4" />
    {:else if name === 'settings'}<path
        d="m9 3-1 3-3 1v3l-2 2 2 2v3l3 1 1 3h6l1-3 3-1v-3l2-2-2-2V7l-3-1-1-3Z"
      /><circle cx="12" cy="12" r="3" />
    {:else if name === 'user'}<circle cx="12" cy="8" r="4" /><path d="M4 21a8 8 0 0 1 16 0" />
    {:else if name === 'trash'}<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7" />
    {:else if name === 'star'}<path
        d="m12 3 3 6 6 1-4.5 4.5 1 6.5-5.5-3-5.5 3 1-6.5L3 10l6-1Z"
      />
    {:else if name === 'edit'}<path d="m16 3 5 5-12 12-6 1 1-6Z M13 6l5 5" />
    {:else if name === 'back'}<path d="m10 5-7 7 7 7M3 12h18" />
    {:else if name === 'info'}<circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7h.01" />
    {:else if name === 'refresh'}<path d="M20 7a9 9 0 1 0 1 9M20 3v5h-5" />
    {/if}
  </svg>
{/if}
