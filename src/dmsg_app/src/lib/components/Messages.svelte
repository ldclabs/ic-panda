<script lang="ts">
  import FormalChannels from './FormalChannels.svelte'
  import LocalMessages from './LocalMessages.svelte'
  import { session } from '../session.svelte'
  let view = $state(session.meta?.account ? 'formal' : 'local')
</script>

<div class="filter-bar" aria-label="消息范围">
  <button class:active={view === 'formal'} onclick={() => (view = 'formal')}>正式频道</button>
  <button class:active={view === 'local'} onclick={() => (view = 'local')}>本机草稿</button>
</div>
{#if view === 'formal'}<FormalChannels />{:else}<LocalMessages />{/if}
