<script lang="ts">
  import CopyButton from '../../components/CopyButton.svelte'
  import Panel from './Panel.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Id from '../../components/page/Id.svelte'
  import { panel } from './workflow-state.svelte'
  import { card, quoteShell } from './model'
  import { openVault } from './navigation.svelte'
  import { call } from '../../rpc'
  import { buttonClass } from '../../components/ui'
  import { workflowText as w } from './workflow-text'
  import { ux, term } from './experience-text'
  let { open }: { open: (id: string) => void } = $props()
  let keys = $state<any[]>([]),
    bindings = $state<any[]>([]),
    ssh = $state<any>({})
  const model = panel(async () =>
    Promise.all([
      model.request('keys', 'secrets/ssh/keys', {}, (r: any) => {
        keys = r.keys
        bindings = r.bindings
      }),
      model.request('status', 'secrets/ssh/status', {}, (r: any) => (ssh = r))
    ])
  )
  const config = (key: any) =>
    `export SSH_AUTH_SOCK=${quoteShell(ssh.socket ?? '')}\ngit config --local gpg.format ssh\ngit config --local user.signingkey ${quoteShell(key.public_key ?? '')}\ngit config --local commit.gpgsign true`
</script>

<Panel {model} title={w('ssh')} description={ux('sshSteps')}>
  <div class={card}>
    <div class="flex flex-wrap justify-between gap-2">
      <span class="text-body"
        >{ux('status')}: {ssh.available ? ux('ready') : ux('unknown')}</span
      ><button
        class={buttonClass('secondary')}
        disabled={model.state.loading}
        onclick={() => model.refresh()}>{ux('sshCheck')}</button
      >
    </div>
    <p class="text-callout text-muted">{ux('sshCheckHint')}</p>
    {#if ssh.socket}<Id
        id={`export SSH_AUTH_SOCK=${quoteShell(ssh.socket)}`}
        full
        wrap
        copyLabel={ux('copyConfig')}
      />{/if}{#if ssh.error || (!ssh.available && model.state.loaded)}<Notice
        tone="warn">{ux('sshUnsupported')}</Notice
      >{/if}
  </div>
  {#if model.state.loaded && !keys.length && !model.state.errors.keys}<div
      class={card}
    >
      <p class="text-body text-muted">{ux('noSources')}</p>
      <button class={buttonClass('primary')} onclick={() => openVault()}
        >{ux('openVault')}</button
      >
    </div>{/if}
  {#each keys as key}<article class={card}>
      <h3 class="font-semibold">{key.title}</h3>
      <Id
        id={key.fingerprint ?? key.reference.item_id}
        full
        wrap
        copyLabel={ux('fingerprint')}
      />{#if key.public_key}<Id
          id={key.public_key}
          full
          wrap
          copyLabel={ux('publicKey')}
        />{/if}{#if !key.supported}<Notice tone="warn"
          >{ux('sshUnsupported')}</Notice
        >{/if}
      <div class="flex flex-wrap gap-2">
        {#each ['ssh_auth', 'git_sign'] as purpose}<button
            class={buttonClass('secondary')}
            disabled={model.state.busy || !key.supported || !ssh.available}
            onclick={() =>
              model.act(
                () =>
                  call('secrets/ssh/binding_request', {
                    reference: key.reference,
                    purposes: [purpose]
                  }),
                (r: any) => open(r.id)
              )}>{w('enable')} · {ux(purpose)}</button
          >{/each}<button
          class={buttonClass('ghost')}
          onclick={() => openVault(key.reference.item_id)}
          >{ux('openSource')}</button
        >
      </div>
      {#if key.supported && ssh.socket}<details>
          <summary class="text-body text-forest"
            >{ux('copyConfig')}</summary
          >
          <pre
            class="my-3 overflow-x-auto whitespace-pre-wrap select-text break-all rounded-lg bg-wash p-3 text-callout"
            dir="ltr">{config(key)}</pre>
          <CopyButton class={buttonClass('secondary')} value={() => config(key)} label={ux('copyConfig')} />
          <p class="mt-2 text-callout text-muted">
            {w('sshEnableWarning')}
            {w('gitWarning')}
          </p>
        </details>{/if}
    </article>{/each}
  {#each bindings.filter((b) => b.enabled) as binding}<article class={card}>
      <p class="text-body">
        {keys.find((k) => k.fingerprint === binding.fingerprint)?.title ??
          ux('fingerprint')}
      </p>
      <Id id={binding.fingerprint} full wrap />
      <p class="text-body">{binding.purposes.map(term).join(' · ')}</p>
      <button
        class={buttonClass('danger')}
        disabled={model.state.busy}
        onclick={() =>
          model.act(() => call('secrets/ssh/disable', { id: binding.id }))}
        >{w('disable')}</button
      >
    </article>{/each}
</Panel>
