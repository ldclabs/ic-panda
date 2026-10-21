<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import { prefersReducedMotion } from './motion'

  // Tool names, inputs and results follow `dmsg mcp`; the data is a sample.
  type Call = { tool: string; input: string; cap: string; result: string }
  type Step =
    | { kind: 'call'; call: Call; state: 'running' | 'ok' | 'denied' }
    | { kind: 'say'; key: string; params?: { capability: string } }
  type Entry = {
    time: string
    who: string
    via: string
    what: string
    ok: boolean
    result: string
  }

  const OUTCOMES: Record<string, string> = {
    dmsg_account_status: 'Account status checked',
    dmsg_vault_read: 'Vault item found',
    dmsg_files_write: 'Release notes saved',
    dmsg_channel_publish: 'Message posted to the channel',
    dmsg_attest_request: 'Waiting for your approval'
  }
  const AGENT = 'claude'
  const DID = 'did:agent:z6Mk…q3Lw'
  const GRANTS = [
    { cap: 'account.read', key: 'Account and sync status' },
    { cap: 'vault.read:deploy', key: 'Read items tagged “deploy”' },
    { cap: 'files.write:fld_r3l', key: 'Write into the “release” folder' },
    { cap: 'channel.publish:release', key: 'Post in the release channel' },
    { cap: 'attest.request', key: 'Ask you to sign a statement' }
  ]
  const NEVER = [
    'Approving any request',
    'Payments and checkout',
    'Backing up, sharing or deleting files',
    'Account and device changes',
    'Pairing browsers',
    'Agent and controller keys'
  ]
  const CALLS: Call[] = [
    {
      tool: 'dmsg_account_status',
      input: '{}',
      cap: 'account.read',
      result: '{"locked":false,"pending_versions":0}'
    },
    {
      tool: 'dmsg_vault_read',
      input: '{"id":"itm_7c1"}',
      cap: 'vault.read:deploy',
      result: '{"title":"Deploy key","type":"api","tags":["deploy"]}'
    },
    {
      tool: 'dmsg_files_write',
      input:
        '{"folder":"fld_r3l","name":"notes-v1.4.md","content_base64url":"IyB2MS40…"}',
      cap: 'files.write:fld_r3l',
      result:
        '{"id":"fil_8k2","version":"ver_2","name":"notes-v1.4.md","versions":2}'
    },
    {
      tool: 'dmsg_channel_publish',
      input: '{"channel":"release","text":"v1.4 ships Friday."}',
      cap: 'channel.publish:release',
      result: '{"seq":42}'
    },
    {
      tool: 'dmsg_attest_request',
      input: '{"title":"Release v1.4","text":"v1.4 is ready to ship."}',
      cap: 'attest.request',
      result: '{"request":"apr_4k2","state":"awaiting_user"}'
    }
  ]

  let grants = $state<Record<string, boolean>>({
    'account.read': true,
    'vault.read:deploy': false,
    'files.write:fld_r3l': true,
    'channel.publish:release': true,
    'attest.request': true
  })
  let steps = $state<Step[]>([])
  let audit = $state<Entry[]>([])
  const completion = $derived(steps.at(-1))
  let running = $state(false)
  let ran = $state(false)
  let copied = $state(false)
  let destroyed = false
  let log: HTMLOListElement

  const COMMAND =
    'claude mcp add dmsg --env DMSG_AGENT_TOKEN=dmsg-agent-… -- dmsg mcp'
  const sleep = (ms: number) =>
    new Promise((resolve) => setTimeout(resolve, ms))
  const now = () => new Date().toTimeString().slice(0, 8)
  const denied = (cap: string) =>
    `{"code":"FORBIDDEN","message":"no grant for ${cap}"}`

  function record(entry: Omit<Entry, 'time'>) {
    audit.unshift({ time: now(), ...entry })
    if (audit.length > 9) audit.length = 9
  }

  function toggle(cap: string) {
    grants[cap] = !grants[cap]
    record({
      who: 'owner',
      via: 'gui',
      what: `${grants[cap] ? 'grants/add' : 'grants/revoke'} ${cap}`,
      ok: true,
      result: 'ok'
    })
  }

  async function follow() {
    await tick()
    // Scroll the transcript, never the page.
    if (log) log.scrollTop = log.scrollHeight
  }

  async function run() {
    if (running) return
    running = true
    ran = true
    steps = []
    const still = prefersReducedMotion()
    const pause = (ms: number) => (still ? Promise.resolve() : sleep(ms))
    for (const call of CALLS) {
      await pause(380)
      if (destroyed) return
      const step: Step = { kind: 'call', call, state: 'running' }
      steps.push(step)
      await follow()
      await pause(520)
      if (destroyed) return
      const allowed = grants[call.cap] ?? false
      const index = steps.length - 1
      steps[index] = { ...step, state: allowed ? 'ok' : 'denied' }
      record({
        who: AGENT,
        via: 'mcp',
        what: call.tool,
        ok: allowed,
        result: allowed ? 'ok' : 'FORBIDDEN'
      })
      if (!allowed)
        steps.push({
          kind: 'say',
          key: 'I don’t have {capability}, so I’ll skip that.',
          params: { capability: call.cap }
        })
      else if (call.tool === 'dmsg_files_write')
        steps.push({
          kind: 'say',
          key: 'Saved as a new version in your library. Backing it up or sharing it stays with you.'
        })
      else if (call.tool === 'dmsg_vault_read')
        steps.push({
          kind: 'say',
          key: 'Found it. The secret itself stays out: that would need vault.secret for this item.'
        })
      await follow()
    }
    await pause(420)
    if (destroyed) return
    steps.push({
      kind: 'say',
      key: grants['attest.request']
        ? 'Done. The release statement is waiting in your approval window. I can’t approve it myself.'
        : 'Done with what I’m allowed to do. Without attest.request I can’t even ask you to sign.'
    })
    await follow()
    running = false
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(COMMAND)
      copied = true
      setTimeout(() => (copied = false), 2000)
    } catch {
      copied = false
    }
  }

  onDestroy(() => {
    destroyed = true
  })
</script>

<div class="connect-cmd">
  <span class="cc-label">{$t('Connect Claude Code')}</span>
  <code>{COMMAND}</code>
  <button class="cc-copy" onclick={copy} aria-label={$t('Copy command')}>
    <Glyph name={copied ? 'check' : 'copy'} size={16} />
    <span>{copied ? $t('Copied') : $t('Copy')}</span>
  </button>
</div>

<div class="agent-lab">
  <aside class="lab-grants">
    <header class="lab-agent">
      <span class="agent-orb"><Glyph name="robot" size={20} /></span>
      <span><strong>{AGENT}</strong><code>{DID}</code></span>
    </header>
    <p class="lab-label">{$t('Granted capabilities')}</p>
    {#each GRANTS as grant}
      <label class="switch-row">
        <input
          type="checkbox"
          role="switch"
          checked={grants[grant.cap]}
          onchange={() => toggle(grant.cap)}
        />
        <span class="switch" aria-hidden="true"></span>
        <span class="sr-text"
          ><code>{grant.cap}</code><small>{$t(grant.key)}</small></span
        >
      </label>
    {/each}
    <p class="lab-label">{$t('Never grantable')}</p>
    <ul class="never">
      {#each NEVER as key}
        <li><Glyph name="forbid" size={14} />{$t(key)}</li>
      {/each}
    </ul>
  </aside>

  <div class="lab-run">
    <div class="lab-task">
      <span class="who">{$t('You')}</span>
      <p>
        {$t(
          'Ship v1.4: check the deploy key, save the release notes, post them, and get the release signed.'
        )}
      </p>
    </div>
    <ol class="lab-steps" bind:this={log}>
      {#each steps as step}
        {#if step.kind === 'call'}
          <li class="call {step.state}">
            <div class="call-head">
              <strong class="call-outcome"
                >{$t(
                  step.state === 'running'
                    ? 'Running…'
                    : step.state === 'denied'
                      ? 'Permission denied'
                      : OUTCOMES[step.call.tool]!
                )}</strong
              >
              <span class="call-state">
                {#if step.state === 'running'}<i class="spin"
                  ></i>{:else if step.state === 'ok'}<Glyph
                    name="check"
                    size={14}
                  />{:else}<Glyph name="forbid" size={14} />{/if}
              </span>
            </div>
            <details class="call-details"
              ><summary>{$t('Show tool details')}</summary>
              <code class="tool">{step.call.tool}</code>
              <code class="call-in">{step.call.input}</code>
              {#if step.state !== 'running'}
                <code class="call-out"
                  >{step.state === 'ok'
                    ? step.call.result
                    : denied(step.call.cap)}</code
                >
              {/if}
            </details>
          </li>
        {:else}
          <li class="say"
            ><span class="who">{AGENT}</span>{$t(step.key, step.params)}</li
          >
        {/if}
      {/each}
    </ol>
    {#if !steps.length}
      <p class="lab-hint">{$t('Toggle a capability, then run the agent.')}</p>
    {/if}
    <p class="sr-only" role="status"
      >{running
        ? $t('Running…')
        : completion?.kind === 'say'
          ? $t(completion.key, completion.params)
          : ''}</p
    >
    <button class="button primary lab-go" onclick={run} disabled={running}>
      <Glyph name={ran ? 'refresh' : 'play'} size={16} />
      {running ? $t('Running…') : ran ? $t('Run again') : $t('Run the agent')}
    </button>
  </div>

  <aside class="lab-audit">
    <p class="lab-label"
      ><Glyph name="history" size={14} />{$t('Audit record')}</p
    >
    <ol>
      {#each audit as entry (entry)}
        <li>
          <time>{entry.time}</time>
          <span class="a-who">{entry.who} · {entry.via}</span>
          <code>{entry.what}</code>
          <b class:ok={entry.ok}>{entry.result}</b>
        </li>
      {:else}
        <li class="a-empty">{$t('Nothing recorded yet.')}</li>
      {/each}
    </ol>
    <p class="lab-foot">
      {$t('It keeps names and outcomes on your device, never content.')}
    </p>
  </aside>
</div>
