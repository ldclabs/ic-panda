<script lang="ts">
  import Policies from './Policies.svelte'
  import Mounts from './Mounts.svelte'
  import Ssh from './Ssh.svelte'
  import Health from './Health.svelte'
  import Sharing from './Sharing.svelte'
  import Receiving from './Receiving.svelte'
  import Activity from './Activity.svelte'
  import { buttonClass } from '../../components/ui'
  import { workflowText as w } from './workflow-text'
  let { open }: { open: (id: string) => void } = $props()
  let tab = $state('policies'),
    target = $state('')
  const tabs = ['policies', 'mounts', 'ssh', 'sharing', 'receive', 'health', 'activity']
  function navigate(next: string, id = '') {
    target = id
    tab = next
  }
</script>

<section class="min-w-0 space-y-6" data-testid="secret-workflows">
  <nav
    class="flex flex-wrap gap-1 border-b border-line pb-4"
    aria-label={w('workflows')}
  >
    {#each tabs as value}<button
        class={buttonClass(tab === value ? 'primary' : 'ghost')}
        aria-current={tab === value ? 'page' : undefined}
        onclick={() => navigate(value)}
        data-testid={`workflow-tab-${value}`}>{w(value)}</button
      >{/each}
  </nav>
  {#if tab === 'policies'}<Policies {open} {navigate} {target} />
  {:else if tab === 'mounts'}<Mounts {open} />
  {:else if tab === 'ssh'}<Ssh {open} />
  {:else if tab === 'health'}<Health {open} {navigate} />
  {:else if tab === 'sharing'}<Sharing {open} />
  {:else if tab === 'receive'}<Receiving {open} />
  {:else}<Activity {open} {target} />{/if}
</section>
