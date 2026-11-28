<script lang="ts">
  import Policies from './Policies.svelte'
  import Mounts from './Mounts.svelte'
  import Ssh from './Ssh.svelte'
  import Health from './Health.svelte'
  import Sharing from './Sharing.svelte'
  import Receiving from './Receiving.svelte'
  import Activity from './Activity.svelte'
  import Links from '../files/Links.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { openVault } from './navigation.svelte'
  import type { NearbyVaultSelection } from '../nearby'

  /** What a stored secret is used for, one part at a time; the vault's filters name the parts. */
  let { open, tab, go, route, account, standalone = false, onNearbyReceive, onNearbySend }: { open: (id: string) => void; tab: string; go: (path: string) => void; route: string; account: string; standalone?: boolean; onNearbyReceive?: () => void; onNearbySend?: (selection: NearbyVaultSelection) => void } = $props()
  const params = $derived(new URLSearchParams(route.split('?')[1]))
  const itemId = $derived(params.get('item') || '')
  const shareMode = $derived<'link' | 'channel' | 'nearby'>(standalone || params.get('mode') === 'nearby' ? 'nearby' : params.get('mode') === 'channel' ? 'channel' : 'link')

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
  {:else if tab === 'sharing'}
    {#key route}
      {#if !standalone && params.has('links')}
        <button class={buttonClass('secondary')} onclick={() => itemId ? openVault(itemId) : go('/vault/sharing')}>{t('back')}</button>
        <Links unlocked vaultOnly {itemId} />
      {:else}
        <Sharing {account} {standalone} initialMode={shareMode} sourceId={itemId} showLinks={(id) => go('/vault/sharing?' + new URLSearchParams(id ? { item: id, links: '1' } : { links: '1' }))} {onNearbySend} />
      {/if}
    {/key}
  {:else if tab === 'receive'}<Receiving {onNearbyReceive} />
  {:else}<Activity {open} {target} />{/if}
</section>
