<script lang="ts">
  import { t } from '$lib/i18n'
  import IconArrowDownLine from '$lib/components/icons/IconArrowDownLine.svelte'
  import InkPainting from './InkPainting.svelte'
  import Registration from './Registration.svelte'

  let sheet: HTMLElement | undefined = $state()
  let painting: ReturnType<typeof InkPainting> | undefined = $state()
  let seed: number | null = $state(null)
</script>

<!--
  The first screen: a sheet of tracing paper on the left, and behind it a
  live ink painting of a panda eating bamboo, which owns the rest.
-->
<section id="hero" class="hero">
  <InkPainting {sheet} bind:seed bind:this={painting} />

  <article class="sheet hero-sheet" bind:this={sheet}>
    <Registration />
    <p class="eyebrow">
      <span class="whitespace-nowrap"
        >{$t('Open Source')} <span class="text-panda">·</span></span
      >
      <span class="whitespace-nowrap"
        >{$t('On-chain Governed')} <span class="text-panda">·</span></span
      >
      <span class="whitespace-nowrap">{$t('Built on ICP')}</span>
    </p>

    <h1 class="display mt-6 text-[clamp(2.1rem,3.4vw,3.5rem)]">
      {$t('From Sovereign Minds')}<br />{$t('to Sovereign Markets.')}<span
        class="caret bg-panda ml-2 inline-block h-[0.58em] w-[0.26em] align-baseline"
        aria-hidden="true"
      ></span>
    </h1>

    <p
      class="text-ink-70 mt-8 max-w-[36em] text-[clamp(1rem,1.25vw,1.2rem)] leading-relaxed text-pretty"
    >
      {$t(
        'ICPanda DAO is an on-chain builder collective creating open infrastructure for persistent AI cognition, verifiable crypto capital formation, and personal control over secrets, identity, and signing.'
      )}
    </p>

    <div class="mt-8 flex flex-col gap-3 sm:flex-row sm:flex-wrap">
      <a class="btn-ink" href="#panda">{$t('Explore PANDA')}</a>
      <a class="btn-outline" href="#projects">
        {$t('What We Build')}
        <span class="*:size-4"><IconArrowDownLine /></span>
      </a>
    </div>

    <p class="text-ink-70 mt-7 max-w-md font-mono text-xs leading-relaxed">
      {$t(
        'Governed by the community through PANDA and the Internet Computer SNS.'
      )}
    </p>

    <!-- Title block, as on a drawing: figure, caption, and the seed it was painted from. -->
    <footer class="tblock">
      <span><b>{$t('fig.00')}</b></span>
      <span>{$t('Panda in bamboo — ink on xuan, painted live')}</span>
      <button
        type="button"
        onclick={() => painting?.repaint()}
        disabled={seed === null}
        title={$t('Repaint')}
      >
        {$t('Seed')}
        {seed === null
          ? '——'
          : seed.toString(16).toUpperCase().padStart(6, '0')} ↻
      </button>
    </footer>
  </article>
</section>

<style>
  .hero {
    display: grid;
    grid-template-columns: minmax(0, min(720px, 52vw)) 1fr;
    align-items: start;
    min-height: calc(100svh - 57px);
    padding: clamp(28px, 6vh, 72px) 12px 48px;
  }

  @media (min-width: 768px) {
    .hero {
      min-height: calc(100svh - 65px);
      padding-inline: max(24px, calc((100% - 72rem) / 2));
    }
  }

  .hero-sheet {
    --film: rgb(248 249 248 / 72%);

    padding: clamp(24px, 3.2vw, 44px) clamp(20px, 3.2vw, 44px) 20px;
    animation: sheet-in 0.7s cubic-bezier(0.22, 1, 0.36, 1) 0.1s both;
  }

  /* Narrow or portrait: she sits above the sheet. */
  @media (max-width: 899px), (max-aspect-ratio: 21/20) {
    .hero {
      grid-template-columns: 1fr;
      padding-top: 44svh;
    }

    .hero-sheet {
      --film: rgb(248 249 248 / 80%);
    }
  }

  @keyframes sheet-in {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
  }

  .tblock {
    display: grid;
    grid-template-columns: auto 1fr auto;
    margin-top: 32px;
    border: 1px solid rgb(11 11 11 / 22%);
    font:
      500 10.5px/1 'IBM Plex Mono',
      ui-monospace,
      monospace;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(11 11 11 / 70%);
  }

  .tblock > * {
    padding: 9px 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tblock > * + * {
    border-inline-start: 1px solid rgb(11 11 11 / 22%);
  }

  .tblock b {
    font-weight: 600;
    color: var(--ink);
  }

  .tblock button {
    font: inherit;
    letter-spacing: inherit;
    text-transform: inherit;
    color: inherit;
    font-variant-numeric: tabular-nums;
    transition:
      background-color 0.2s,
      color 0.2s;
  }

  .tblock button:not(:disabled):hover {
    background: var(--ink);
    color: var(--paper);
  }

  @media (prefers-reduced-motion: reduce) {
    .hero-sheet {
      animation: none;
    }
  }
</style>
