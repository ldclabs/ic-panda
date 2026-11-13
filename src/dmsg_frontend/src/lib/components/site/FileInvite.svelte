<script lang="ts">
  import Glyph from '$lib/components/landing/Glyph.svelte'
  import { installFor } from '$lib/downloads'
  import { t } from '$lib/i18n'

  let { ready = false, kind }: { ready?: boolean; kind: 'share' | 'request' } =
    $props()
  const install = installFor(navigator.userAgent, navigator.maxTouchPoints)
</script>

<aside
  class="invite"
  class:ready
  aria-labelledby="invite-title"
  data-testid={`${kind}-invite`}
>
  <h2 id="invite-title">{$t('Share files privately. Collect them too.')}</h2>
  <ul>
    <li
      ><Glyph name="check" size={16} /><span
        >{$t(
          'Share a file by link that can expire, ask for a password or be revoked.'
        )}</span
      ></li
    >
    <li
      ><Glyph name="check" size={16} /><span
        >{$t(
          'Collect files with a private upload link. Only you can open what arrives. Available on Plus and above.'
        )}</span
      ></li
    >
    <li
      ><Glyph name="check" size={16} /><span
        >{$t('Recipients and uploaders need no account or app.')}</span
      ></li
    >
  </ul>
  <div class="invite-actions">
    <a
      class="button primary"
      href={install.href}
      data-testid={`${kind}-install`}
      data-sveltekit-reload
      ><Glyph name="download" size={18} />{$t(install.label)}</a
    >
    <a class="button secondary" href="/#collaboration" data-sveltekit-reload
      >{$t('See how dMsg works')}</a
    >
  </div>
</aside>

<style>
  .invite {
    margin-top: 16px;
    padding: 24px 32px 28px;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--pale);
    font-size: 14px;
  }
  .invite.ready {
    border-color: var(--action);
  }
  .invite h2 {
    margin: 0 0 8px;
    font-size: 20px;
    font-weight: 650;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  ul {
    display: grid;
    gap: 12px;
    margin: 16px 0 20px;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr);
    gap: 8px;
    align-items: start;
    color: var(--ink);
  }
  li :global(svg) {
    margin-top: 3px;
    color: var(--action);
  }
  .invite-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .invite-actions .button {
    flex: 1 1 200px;
    min-width: 0;
    white-space: normal;
    text-align: center;
  }
  @media (max-width: 540px) {
    .invite {
      padding: 20px;
    }
  }
</style>
