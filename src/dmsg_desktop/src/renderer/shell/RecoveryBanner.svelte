<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { network } from '../network.svelte'
  import Icon from '../components/Icon.svelte'

  onMount(() => {
    let alive = true
    const read = async () => {
      try {
        const connection = await window.dmsg.connection()
        if (alive) network.state = connection.connected ? connection.recovery ?? 'idle' : 'recovering'
      } catch { if (alive) network.state = 'offline' }
    }
    void read()
    const off = window.dmsg.onEvent((event) => {
      if (event.type === 'connection/changed') void read()
    })
    return () => { alive = false; off(); network.state = 'idle' }
  })
</script>

{#if network.state !== 'idle'}
  <div class="flex shrink-0 items-center gap-2 border-b border-line bg-warn-pale px-6 py-2 text-[12.5px] text-ink-2" role="status" data-testid="network-recovery">
    <Icon name="sync" class={network.state === 'recovering' ? 'motion-safe:animate-spin' : ''} />
    <span>{t(network.state === 'recovering' ? 'networkReconnecting' : 'networkUnavailable')}</span>
  </div>
{/if}
