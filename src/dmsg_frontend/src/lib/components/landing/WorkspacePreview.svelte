<script lang="ts">
  import { tick } from 'svelte'
  import Brand from './Brand.svelte'
  import Icon from './Icon.svelte'

  let { review }: { review: () => void } = $props()
  const tabs = ['Keep it private', 'Share with intent', 'Sign with clarity']
  let selected = $state(0)
  let revealed = $state(false)
  let locked = $state(false)
  let feedback = $state('')
  const secret = 'dmsg_demo_key_not_a_secret'
  const sample =
    'dMsg release brief\n\nPurpose: confirm this example file version.\nSample content only. No real release or approval is represented.\n'

  function selectWithKey(event: KeyboardEvent) {
    let next = selected
    if (event.key === 'ArrowRight') next = (selected + 1) % tabs.length
    else if (event.key === 'ArrowLeft')
      next = (selected + tabs.length - 1) % tabs.length
    else if (event.key === 'Home') next = 0
    else if (event.key === 'End') next = tabs.length - 1
    else return
    event.preventDefault()
    selected = next
    document.getElementById(`task-${next}`)?.focus()
  }

  async function setLocked(value: boolean) {
    locked = value
    revealed = false
    feedback = ''
    await tick()
    document.getElementById(value ? 'unlock-example' : 'lock-example')?.focus()
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(secret)
      feedback = 'Sample copied to your system clipboard.'
    } catch {
      revealed = true
      feedback =
        'Clipboard unavailable. Select and copy the sample value above.'
    }
  }

  function download() {
    const url = URL.createObjectURL(new Blob([sample], { type: 'text/plain' }))
    const link = document.createElement('a')
    link.href = url
    link.download = 'dmsg-sample-release-brief.txt'
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
  }
</script>

<section id="workspace" aria-label="Explore dMsg capabilities">
  <div class="task-tabs" role="tablist" aria-label="Workspace capabilities">
    {#each tabs as tab, index}
      <button
        id={`task-${index}`}
        role="tab"
        aria-selected={selected === index}
        aria-controls={`panel-${index}`}
        tabindex={selected === index ? 0 : -1}
        onclick={() => {
          selected = index
          feedback = ''
        }}
        onkeydown={selectWithKey}
      >
        <span>0{index + 1}</span>{tab}
      </button>
    {/each}
  </div>

  {#each tabs as _, index}
    <div
      id={`panel-${index}`}
      role="tabpanel"
      aria-labelledby={`task-${index}`}
      tabindex="0"
      hidden={selected !== index}
    >
      <div class="workspace-panel">
        <div class="story">
          <div class="eyebrow"
            >{[
              'Your personal vault',
              'Your chosen people',
              'Your explicit approval'
            ][index]}</div
          >
          <h2
            >{[
              'A place for the things you don’t publish.',
              'The right content. The right people.',
              'Know exactly what you’re signing.'
            ][index]}</h2
          >
          <p
            >{[
              'Keep API keys, credentials, private notes, and files together. The planned extension puts your personal workspace first — no contacts or paid handle required.',
              'Share project notes and files in an encrypted conversation or channel. Invite people into a specific space, with access separate from your personal vault.',
              'Review the requesting app, your identity, the exact file version, and the purpose. Connecting an app is a separate decision from approving its signature request.'
            ][index]}</p
          >
          <div class="tags">
            {#each [['Private notes', 'Credentials', 'Local search'], ['Invited members', 'Encrypted files', 'Scoped history'], ['Fixed file version', 'Clear purpose', 'Separate permissions']][index] as tag}
              <span>{tag}</span>
            {/each}
          </div>
        </div>
        <div class="demo">
          <div class="demo-top"
            ><Brand /><span class="demo-label">Interactive example</span></div
          >
          <div class="demo-body">
            {#if index === 0}
              {#if locked}
                <div class="locked">
                  <span class="icon-box"><Icon name="lock" /></span>
                  <h3>Example vault locked</h3>
                  <p
                    >A real vault will require an authorized device and local
                    unlock.</p
                  >
                  <button
                    class="button primary"
                    id="unlock-example"
                    onclick={() => setLocked(false)}>Unlock example</button
                  >
                </div>
              {:else}
                <div class="item-heading"
                  ><span class="icon-box"><Icon name="key" /></span><div
                    ><h3>Production API key</h3><p
                      >Example project · Sample credential</p
                    ></div
                  ></div
                >
                <div class="data-row secret-row"
                  ><span>Value</span><div class="secret"
                    ><code>{revealed ? secret : '•••• •••• •••• ••••'}</code
                    ><button
                      class="icon-button"
                      aria-label={revealed
                        ? 'Hide sample secret'
                        : 'Show sample secret'}
                      aria-pressed={revealed}
                      onclick={() => (revealed = !revealed)}
                      ><Icon name="eye" /></button
                    ></div
                  ></div
                >
                <div class="data-row"
                  ><span>Access</span><strong>Personal vault</strong></div
                >
                <div class="demo-actions"
                  ><button
                    class="button secondary"
                    id="lock-example"
                    onclick={() => setLocked(true)}
                    ><Icon name="lock" />Lock example</button
                  ><button class="button primary" onclick={copy}
                    ><Icon name="copy" />Copy sample</button
                  ></div
                >
                <p class="demo-help"
                  >Sample data only. Never enter a real secret on this page.</p
                >
              {/if}
            {:else if index === 1}
              <div class="item-heading"
                ><span class="icon-box"><Icon name="chat" /></span><div
                  ><h3>Release room</h3><p
                    >Example channel · 2 invited members</p
                  ></div
                ></div
              >
              <div class="message"
                ><strong>Alex · Example message</strong><p
                  >Here’s the release brief for your review. Let’s agree on this
                  version before publishing.</p
                ></div
              >
              <div class="file-row"
                ><Icon name="file" /><div
                  ><strong>release-brief.txt</strong><p>Sample text file</p
                  ></div
                ><button
                  class="icon-button"
                  aria-label="Download sample release brief"
                  onclick={download}><Icon name="download" /></button
                ></div
              >
              <p class="demo-help"
                >New members do not automatically receive old history. Removing
                access cannot erase copies already downloaded.</p
              >
            {:else}
              <div class="item-heading"
                ><span class="icon-box"><Icon name="signature" /></span><div
                  ><h3>Confirm a release brief</h3><p
                    >Example signature request</p
                  ></div
                ></div
              >
              <div class="data-row"
                ><span>Requesting app</span><strong>TokenList · Example</strong
                ></div
              >
              <div class="data-row"
                ><span>File</span><strong>release-brief.txt · v1</strong></div
              >
              <div class="data-row"
                ><span>Purpose</span><strong>Confirm this file version</strong
                ></div
              >
              <div class="demo-actions"
                ><button class="button primary" onclick={review}
                  >Review example<Icon name="arrow-right" /></button
                ></div
              >
              <p class="demo-help"
                >Walkthrough only. No identity connection or real signature.</p
              >
            {/if}
            <p class="feedback" role="status"
              >{index === selected ? feedback : ''}</p
            >
          </div>
        </div>
      </div>
    </div>
  {/each}
  <div class="task-caption"
    ><span>Product preview · Chrome extension in development</span><span
      >Examples run locally. Nothing is sent or signed.</span
    ></div
  >
  <div class="capability-notes">
    <div
      ><h3>Useful on your own.</h3><p
        >Start with one private note or credential. A public identity is an
        optional doorway, not a prerequisite.</p
      ></div
    >
    <div
      ><h3>Share a specific space.</h3><p
        >A channel invitation grants access to that channel. It does not open
        your personal vault.</p
      ></div
    >
    <div
      ><h3>Every permission has a purpose.</h3><p
        >Login, content access, device approval, and formal signing are separate
        decisions.</p
      ></div
    >
  </div>
</section>
