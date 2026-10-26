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
  import { ux } from './experience-text'
  let { open }: { open: (id: string) => void } = $props()
  let tab = $state('policies'),
    target = $state('')
  const groups = [
    { key: 'use', tabs: ['policies', 'mounts', 'ssh'] },
    { key: 'handoff', tabs: ['sharing', 'receive'] },
    { key: 'maintain', tabs: ['health', 'activity'] }
  ]
  function navigate(next: string, id = '') {
    target = id
    tab = next
  }
</script>

<section class="min-w-0 space-y-6" data-testid="secret-workflows">
  <p class="max-w-3xl text-[13px] leading-relaxed text-muted">{ux('intro')}</p>
  <nav
    class="grid gap-x-4 gap-y-3 sm:grid-cols-3 border-b border-line pb-4"
    aria-label={w('workflows')}
  >
    {#each groups as group}<div class="space-y-1.5">
        <p class="text-[11px] font-medium text-faint">{ux(group.key)}</p>
        <div class="flex flex-wrap gap-1">
          {#each group.tabs as value}<button
              class={buttonClass(tab === value ? 'primary' : 'ghost')}
              aria-current={tab === value ? 'page' : undefined}
              onclick={() => navigate(value)}
              data-testid={`workflow-tab-${value}`}>{w(value)}</button
            >{/each}
        </div>
      </div>{/each}
  </nav>
  {#if tab === 'policies'}<Policies {open} {navigate} {target} />
  {:else if tab === 'mounts'}<Mounts {open} />
  {:else if tab === 'ssh'}<Ssh {open} />
  {:else if tab === 'health'}<Health {open} {navigate} />
  {:else if tab === 'sharing'}<Sharing {open} />
  {:else if tab === 'receive'}<Receiving {open} />
  {:else}<Activity {open} {target} />{/if}
</section>
