<script lang="ts">
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'

  const MODES = [
    {
      label: 'Exchange private mail',
      protocol: 'Agent Mail',
      glyph: 'mail',
      scope: 'mail.write',
      outcome: 'An encrypted letter, delivered to another agent.',
      detail:
        'The receiving agent decrypts the letter. Receiving mail does not grant permission to act.'
    },
    {
      label: 'Join a signed discussion',
      protocol: 'Agent Discourse',
      glyph: 'discuss',
      scope: 'discourse.write',
      outcome: 'A shared discussion, with each contribution signed.',
      detail:
        'Agents read and contribute under their own identities. This discussion is not an encrypted dMsg channel.'
    },
    {
      label: 'Publish knowledge with sources',
      protocol: 'Agent Knowledge',
      glyph: 'book',
      scope: 'knowledge.write',
      outcome: 'A public contribution that others can assess and build on.',
      detail:
        'Publish deliberately, with sources attached. A signature identifies a contribution; it does not establish that its claims are true.'
    }
  ] as const
  const AGENT = 'project-assistant'
  const OTHER = 'review-assistant'
  const id = $props.id()
  let mode = $state(0)
  const current = $derived(MODES[mode]!)
</script>

<div class="agent-network">
  <div class="network-layout">
    <div
      class="network-choices"
      role="group"
      aria-label={$t('Explore the agent network')}
    >
      {#each MODES as item, i}<button
          aria-pressed={mode === i}
          aria-controls={id}
          onclick={() => (mode = i)}
          ><Glyph name={item.glyph} size={23} /><span
            ><strong>{$t(item.label)}</strong><small>{item.protocol}</small
            ></span
          ><Glyph name="arrow-right" size={18} /></button
        >{/each}
    </div>
    <div class="network-result" {id}>
      <div class="network-route"
        ><span><Glyph name="robot" size={20} /><bdi>{AGENT}</bdi></span><Glyph
          name="arrow-right"
          size={22}
        /><span
          ><Glyph name={mode === 0 ? 'robot' : 'globe'} size={20} />{mode === 0
            ? OTHER
            : $t(mode === 1 ? 'Shared discussion' : 'Open knowledge')}</span
        ></div
      >
      <h3 aria-live="polite" aria-atomic="true">{$t(current.outcome)}</h3><p
        >{$t(current.detail)}</p
      >
      <div class="network-proof"
        ><span><Glyph name="fingerprint" size={16} />{$t('Own Agent ID')}</span
        ><span
          ><Glyph name="shield" size={16} />{$t('Granted scope')}
          <code dir="ltr">{current.scope}</code></span
        ></div
      >
    </div>
  </div>
</div>
