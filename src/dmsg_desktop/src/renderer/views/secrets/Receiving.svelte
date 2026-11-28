<script lang="ts">
  import { onDestroy } from 'svelte'
  import Panel from './Panel.svelte'
  import PackageImport from './PackageImport.svelte'
  import Icon from '../../components/Icon.svelte'
  import SecretInput from '../../components/SecretInput.svelte'
  import { panel } from './workflow-state.svelte'
  import { card, DraftVersion } from './model'
  import { call } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  let { onNearbyReceive }: { onNearbyReceive?: () => void } = $props()
  let link = $state(''),
    password = $state(''),
    received = $state<any>(null)
  const draft = new DraftVersion(),
    model = panel(async () => {})
  function changed() {
    draft.change()
    received = null
  }
  async function preview(e: SubmitEvent) {
    e.preventDefault()
    const at = draft.capture(),
      selected = link,
      secret = password
    await model.act(
      () =>
        call('secrets/packages/import_preview', {
          link: selected,
          ...(secret ? { password: secret } : {})
        }),
      (r: any) => {
        if (draft.current(at) && link === selected && password === secret) {
          received = r
          link = ''
          password = ''
        }
      },
      false
    )
  }
  onDestroy(() => {
    draft.change()
    link = ''
    password = ''
    received = null
  })
</script>

<Panel {model} title={w('receive')} description={w('sessionNotice')}>
  {#if onNearbyReceive}
    <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-line bg-surface p-3.5">
      <div class="min-w-0 flex-1">
        <p class="text-body font-medium text-ink">{t('nearbyReceive')}</p>
        <p class="text-callout text-muted">{t('nearbyReceiveHint')}</p>
      </div>
      <button type="button" class={buttonClass('secondary', 'sm')} onclick={onNearbyReceive} data-testid="receive-nearby-open">
        <Icon name="pair" />{t('nearbyReceive')}
      </button>
    </div>
  {/if}
  <form class={card} onsubmit={preview} oninput={changed}>
    <SecretInput label={w('link')} bind:value={link} />
    <SecretInput label={w('password')} bind:value={password} />
    <button class={buttonClass('primary')} disabled={model.state.busy || !link}
      >{w('preview')}</button
    >
  </form>
  {#if received}{#key received.package_id}<PackageImport
        {received}
      />{/key}{/if}
</Panel>
