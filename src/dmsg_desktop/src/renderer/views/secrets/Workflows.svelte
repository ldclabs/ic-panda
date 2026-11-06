<script lang="ts">
  import Policies from './Policies.svelte'
  import Mounts from './Mounts.svelte'
  import Ssh from './Ssh.svelte'
  import Health from './Health.svelte'
  import Sharing from './Sharing.svelte'
  import Receiving from './Receiving.svelte'
  import Activity from './Activity.svelte'

  /** What a stored secret is used for, one part at a time; the vault's filters name the parts. */
  let { open, tab, go }: { open: (id: string) => void; tab: string; go: (path: string) => void } = $props()

  let target = $state('')

  function navigate(next: string, id = '') {
    target = id
    go(`/vault/${next}`)
  }
</script>

<section class="min-w-0 space-y-6" data-testid="secret-workflows">
  {#if tab === 'policies'}<Policies {open} {navigate} {target} />
  {:else if tab === 'mounts'}<Mounts {open} />
  {:else if tab === 'ssh'}<Ssh {open} />
  {:else if tab === 'health'}<Health {open} {navigate} />
  {:else if tab === 'sharing'}<Sharing {open} />
  {:else if tab === 'receive'}<Receiving {open} />
  {:else}<Activity {open} {target} />{/if}
</section>
