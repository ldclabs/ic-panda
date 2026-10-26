<script lang="ts">
  import { onDestroy } from 'svelte'
  import Panel from './Panel.svelte'
  import PackageImport from './PackageImport.svelte'
  import SecretInput from '../../components/SecretInput.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { panel } from './workflow-state.svelte'
  import { card, DraftVersion } from './model'
  import { call } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { workflowText as w } from './workflow-text'
  let { open }: { open: (id: string) => void } = $props()
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
  <form class={card} onsubmit={preview} oninput={changed}>
    <Field label={w('link')}><SecretInput bind:value={link} /></Field><Field
      label={w('password')}><SecretInput bind:value={password} /></Field
    >
    <button class={buttonClass('primary')} disabled={model.state.busy || !link}
      >{w('preview')}</button
    >
  </form>
  {#if received}{#key received.package_id}<PackageImport
        {received}
        {open}
      />{/key}{/if}
</Panel>
