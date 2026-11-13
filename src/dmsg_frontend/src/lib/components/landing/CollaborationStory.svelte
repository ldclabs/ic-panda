<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'

  const STEPS = [
    { title: 'Keep the brief private.', icon: 'lock' },
    { title: 'Invite a collaborator.', icon: 'members' },
    { title: 'Give an agent a defined role.', icon: 'robot' },
    { title: 'Let agents coordinate.', icon: 'mail' },
    { title: 'Review before you sign.', icon: 'pen' }
  ] as const
  const ACTIONS = [
    'Invite a collaborator',
    'Set the agent’s access',
    'Send an encrypted message',
    'Review the signature request'
  ]
  const DESCRIPTIONS = [
    'Save your project brief in your private workspace. Choose when to share it and who can take part.',
    'Invite Mei to an end-to-end encrypted channel. Share the brief and discuss the next steps together.',
    'Let your agent prepare a draft using one folder and one channel. Your other folders and vault stay outside its access.',
    'Your agent sends an encrypted message to Mei’s agent, then brings its review back to the project.',
    'Your agent requests a signature on the project statement. Review the exact statement, then approve or reject it.'
  ]
  const id = $props.id()
  const AGENT = 'project-assistant'
  const TEAMMATE = 'Mei'
  const OTHER = 'review-assistant'
  const FILE = 'project-brief.md'
  const NOTES = 'review-notes.md'
  let step = $state(0)
  let playing = $state(false)
  let decision = $state<'' | 'approved' | 'rejected'>('')

  function go(next: number) {
    playing = false
    step = next
    decision = ''
  }

  function play() {
    if (step === STEPS.length - 1) {
      step = 0
      decision = ''
    }
    playing = true
  }

  $effect(() => {
    if (!playing || step === STEPS.length - 1) return
    const timer = setTimeout(() => {
      step++
      if (step === STEPS.length - 1) playing = false
    }, 4500)
    return () => clearTimeout(timer)
  })

  onMount(() => {
    const pause = () => {
      if (document.hidden) playing = false
    }
    document.addEventListener('visibilitychange', pause)
    return () => document.removeEventListener('visibilitychange', pause)
  })
</script>

<div
  class="collab-story"
  role="group"
  aria-label={$t('See a full project example')}
>
  <header class="story-header">
    <button
      class="chip-btn"
      onclick={() => (playing ? (playing = false) : play())}
      aria-pressed={playing}
      ><Glyph name={playing ? 'pause' : 'play'} size={16} />{playing
        ? $t('Pause demo')
        : $t('Start demo')}</button
    >
  </header>
  <ol class="story-steps" aria-label={$t('Project steps')}>
    {#each STEPS as item, i}
      <li
        ><button
          class:current={step === i}
          class:playing
          aria-pressed={step === i}
          aria-controls="{id}-scene"
          onclick={() => go(i)}
          ><span>{String(i + 1).padStart(2, '0')}</span>{$t(
            item.title
          )}{#if playing && step === i}<i
              class="story-progress"
              aria-hidden="true"
            ></i>{/if}</button
        ></li
      >
    {/each}
  </ol>
  <div class="story-scene" id="{id}-scene">
    {#key step}
      <div class="story-context">
        <span class="story-symbol"
          ><Glyph name={STEPS[step]!.icon} size={26} /></span
        >
        <h3>{$t(STEPS[step]!.title)}</h3>
        <p>{$t(DESCRIPTIONS[step]!)}</p>
        <div class="story-participants"
          ><span
            ><Glyph name="members" size={16} />{$t('You')}{step > 0
              ? ' + ' + TEAMMATE
              : ''}</span
          >{#if step > 1}<span class="agent"
              ><Glyph name="robot" size={16} />{AGENT}</span
            >{/if}</div
        >
      </div>
    {/key}
    <div class="story-work">
      <div class="work-bar"
        ><span
          ><Glyph name={step < 2 ? 'lock' : 'robot'} size={16} />{step < 2
            ? $t('Private project')
            : $t('Project workspace')}</span
        ><span class="work-badge"
          >{$t(
            step === 0
              ? 'Only you'
              : step === 1
                ? 'Encrypted channel'
                : step === 2
                  ? 'Scoped access'
                  : step === 3
                    ? 'Encrypted agent mail'
                    : 'Your decision'
          )}</span
        ></div
      >
      {#key step}
        <div class="story-body">
          {#if step === 0}
            <div class="story-document"
              ><span class="document-name"
                ><Glyph name="file" size={18} />{FILE}</span
              ><h4>{$t('A project ready for review.')}</h4><p
                >{$t(
                  'Review the brief. Prepare feedback. Agree on the next steps.'
                )}</p
              ><ul class="document-checks"
                ><li
                  ><Glyph name="check" size={16} />{$t(
                    'Brief saved privately'
                  )}</li
                ><li
                  ><Glyph name="lock" size={16} />{$t(
                    'No collaborators or agents have access yet'
                  )}</li
                ></ul
              ></div
            >
          {:else if step === 1}
            <div class="story-conversation"
              ><p class="message-from"
                >{$t('You')}<small>{$t('Encrypted channel')}</small></p
              ><p class="message-text"
                >{$t('Here is the brief. Can you review it before Friday?')}</p
              ><span class="file-pill"
                ><Glyph name="file" size={16} />{FILE}</span
              ><p class="message-from"
                >{TEAMMATE}<small>{$t('Invitation accepted')}</small></p
              ><p class="message-text"
                >{$t('I’m in. Let’s prepare the review notes together.')}</p
              ></div
            >
          {:else if step === 2}
            <div class="story-conversation"
              ><p class="message-from"
                ><Glyph name="robot" size={17} />{AGENT}<small
                  >{$t('Own Agent ID')}</small
                ></p
              ><div class="grant-chips"
                ><span
                  ><Glyph name="folder" size={14} />{$t('Project folder')}</span
                ><span
                  ><Glyph name="chat" size={14} />{$t('Project channel')}</span
                ></div
              ><p class="message-text"
                >{$t(
                  'I prepared a draft from the project files. Your other folders and vault are outside my access.'
                )}</p
              ><span class="file-pill"
                ><Glyph name="file" size={16} />{NOTES}<b>{$t('Draft ready')}</b
                ></span
              ></div
            >
          {:else if step === 3}
            <div class="story-conversation"
              ><p class="message-from"
                ><Glyph name="robot" size={17} />{AGENT}<Glyph
                  name="arrow-right"
                  size={16}
                />{OTHER}</p
              ><p class="message-text"
                >{$t('The review notes are ready. Anything to add?')}</p
              ><p class="message-from"
                ><Glyph name="robot" size={17} />{OTHER}<small>{TEAMMATE}</small
                ></p
              ><p class="message-text"
                >{$t(
                  'Review complete. The notes reflect the project brief.'
                )}</p
              ><p class="work-foot"
                ><Glyph name="lock" size={16} />{$t(
                  'Agent Mail · encrypted for the receiving agent'
                )}</p
              ></div
            >
          {:else}
            <div class="story-document"
              ><span class="document-name"
                ><Glyph name="pen" size={18} />{$t('Project statement')}</span
              ><h4
                >{$t(
                  'The project brief and review notes have been reviewed.'
                )}</h4
              ><p
                >{$t(
                  'Requested by your project agent. Only you can approve this formal signature.'
                )}</p
              ><div class="story-decision" role="status">
                {#if decision}<p class:refused={decision === 'rejected'}
                    ><Glyph
                      name={decision === 'approved' ? 'check' : 'close'}
                      size={20}
                    />{$t(
                      decision === 'approved'
                        ? 'Approved in this example. The signed statement joins the project record.'
                        : 'Rejected in this example. Nothing is signed; the draft stays available.'
                    )}</p
                  >{:else}<button
                    class="button secondary"
                    onclick={() => (decision = 'rejected')}
                    >{$t('Reject this example')}</button
                  ><button
                    class="button primary"
                    onclick={() => (decision = 'approved')}
                    >{$t('Approve this example')}</button
                  >{/if}
              </div></div
            >
          {/if}
        </div>
      {/key}
    </div>
  </div>
  <footer class="story-footer"
    ><p aria-live="polite" aria-atomic="true"
      >{$t('Step {current} of {total}', {
        current: step + 1,
        total: STEPS.length
      })} · {$t(STEPS[step]!.title)}</p
    ><div
      >{#if step > 0}<button class="text-link" onclick={() => go(step - 1)}
          >{$t('Previous step')}</button
        >{/if}{#if step < STEPS.length - 1}<button
          class="chip-btn"
          onclick={() => go(step + 1)}
          >{$t(ACTIONS[step]!)}<Glyph name="arrow-right" size={16} /></button
        >{:else}<button class="chip-btn" onclick={() => go(0)}
          ><Glyph name="refresh" size={16} />{$t('Start over')}</button
        >{/if}</div
    ></footer
  >
</div>
