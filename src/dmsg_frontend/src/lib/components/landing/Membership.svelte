<script lang="ts">
  import { locale, t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import { reveal } from './motion'

  // Public marketing snapshot, checked 2026-10-28: effective commerce catalog
  // v2 (uwist-sqaaa-aaaaj-qsjyq-cai), wallet hub get_hub_config
  // (ghcje-dyaaa-aaaas-qg7va-cai), and dmsg_types::billing::PlanId::device_limit.
  // Recheck both services when updating these figures; Rust's default catalog
  // is not the live catalog. Checkout always confirms current terms in the app.
  const plans = [
    {
      name: 'Free',
      usd: 0,
      storage: 100,
      unit: 'MiB',
      devices: 2,
      channels: 5,
      signatures: 5,
      wallets: 0,
      audience: 'For everyday essentials.'
    },
    {
      name: 'Plus',
      usd: 10,
      storage: 1,
      unit: 'GiB',
      devices: 3,
      channels: 20,
      signatures: 20,
      wallets: 1,
      audience: 'For a growing personal workspace.'
    },
    {
      name: 'Pro',
      usd: 50,
      storage: 10,
      unit: 'GiB',
      devices: 5,
      channels: 100,
      signatures: 100,
      wallets: 3,
      audience: 'For work and collaboration.'
    },
    {
      name: 'Max',
      usd: 200,
      storage: 100,
      unit: 'GiB',
      devices: 10,
      channels: 10_000,
      signatures: 10_000,
      wallets: 10,
      audience: 'For larger teams and workloads.'
    }
  ] as const

  const number = $derived(new Intl.NumberFormat($locale))
  const price = $derived(
    new Intl.NumberFormat($locale, {
      style: 'currency',
      currency: 'USD',
      currencyDisplay: 'narrowSymbol',
      maximumFractionDigits: 0
    })
  )
  const checkedAt = $derived(
    new Intl.DateTimeFormat($locale, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      timeZone: 'UTC'
    }).format(new Date('2026-10-28T00:00:00Z'))
  )
</script>

<section
  class="section membership band"
  id="membership"
  aria-labelledby="membership-title"
>
  <div class="wrap">
    <header class="sec-head" use:reveal>
      <p class="kicker"><b>06</b>{$t('Membership')}</p>
      <h2 id="membership-title"
        >{$t('Start free.')}<br /><span class="soft"
          >{$t('Room to do more.')}</span
        ></h2
      >
      <p class="sec-lead"
        >{$t('One private workspace, with more capacity when you need it.')}</p
      >
    </header>

    <div class="plans" use:reveal>
      {#each plans as plan}
        <article class="plan" aria-labelledby={`membership-${plan.name}`}>
          <header class="plan-heading">
            <h3 id={`membership-${plan.name}`}><bdi>{plan.name}</bdi></h3>
            <p class="audience">{$t(plan.audience)}</p>
            <p class="price"
              ><bdi>{price.format(plan.usd)}</bdi><span>{$t('USD / year')}</span
              ></p
            >
          </header>
          <dl>
            <div class="storage"
              ><dt>{$t('Encrypted cloud storage')}</dt><dd
                ><bdi>{number.format(plan.storage)} {plan.unit}</bdi></dd
              ></div
            >
            <div
              ><dt>{$t('Active devices')}</dt><dd
                >{number.format(plan.devices)}</dd
              ></div
            >
            <div
              ><dt>{$t('Active channels')}</dt><dd
                >{number.format(plan.channels)}</dd
              ></div
            >
            <div
              ><dt>{$t('Formal signatures / month')}</dt><dd
                >{number.format(plan.signatures)}</dd
              ></div
            >
            <div
              ><dt>{$t('Team wallet slots')}</dt><dd
                >{number.format(plan.wallets)}</dd
              ></div
            >
          </dl>
        </article>
      {/each}
    </div>

    <div class="membership-details" use:reveal>
      <div class="included">
        <Glyph name="lock" size={22} />
        <div
          ><h3>{$t('Privacy comes with every plan.')}</h3><p
            >{$t(
              'Encryption, device revocation and account recovery are included at every tier.'
            )}</p
          ></div
        >
      </div>
      <div class="subscribe">
        <h3>{$t('Choose your plan in the app.')}</h3>
        <p
          >{$t(
            'Pay with ckUSDC or ckUSDT, or use an eligible PANDA neuron for the annual membership.'
          )}</p
        >
        <a class="button primary" href="#release"
          >{$t('Download dMsg')}<Glyph name="arrow-right" /></a
        >
      </div>
    </div>
    <div class="plan-notes">
      <p
        >{$t(
          'Channel limits apply to channels you create or sponsor. Ordinary device signatures do not use the monthly allowance.'
        )}</p
      >
      <p
        >{$t(
          'Wallet activation is charged separately; check availability in the app.'
        )}</p
      >
      <p
        >{$t(
          'Prices and limits checked on {date}. Confirm current terms in the app.',
          { date: checkedAt }
        )}</p
      >
    </div>
  </div>
</section>

<style>
  .plans {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    border-block: 1px solid var(--line-2);
  }
  .plan {
    display: grid;
    grid-row: span 6;
    grid-template-rows: subgrid;
    min-width: 0;
    padding: 32px 24px;
  }
  .plan + .plan {
    border-inline-start: 1px solid var(--line-2);
  }
  .plan:first-child {
    padding-inline-start: 0;
  }
  .plan:last-child {
    padding-inline-end: 0;
  }
  .plan-heading {
    display: grid;
    grid-template-rows: auto minmax(48px, auto) auto;
  }
  .plan h3 {
    margin: 0 0 8px;
    font-size: 23px;
    font-weight: 650;
    letter-spacing: -0.025em;
  }
  .audience {
    margin: 0;
    color: var(--muted);
    font-size: 14px;
    line-height: 1.6;
  }
  .price {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
    margin: 24px 0 28px;
  }
  .price bdi {
    color: var(--forest);
    font-size: clamp(32px, 3vw, 44px);
    font-weight: 650;
    line-height: 1.1;
    letter-spacing: -0.04em;
    font-variant-numeric: tabular-nums;
  }
  .price span {
    color: var(--muted);
    font-size: 12px;
  }
  dl {
    display: grid;
    grid-row: span 5;
    grid-template-rows: subgrid;
    margin: 0;
  }
  dl > div {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 16px;
    padding: 12px 0;
    border-top: 1px solid var(--line);
    font-size: 14px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    flex-shrink: 0;
    margin: 0;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .storage {
    flex-direction: column;
    gap: 2px;
    padding-top: 0;
    padding-bottom: 20px;
    border: 0;
  }
  .storage dd {
    font-size: 22px;
  }
  .membership-details {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 64px;
    padding: 36px 0;
  }
  .membership-details h3 {
    margin: 0 0 8px;
    font-size: 18px;
    font-weight: 600;
    line-height: 1.4;
  }
  .membership-details p {
    margin: 0;
    color: var(--muted);
    font-size: 15px;
    line-height: 1.65;
    max-width: 480px;
  }
  .included {
    display: flex;
    gap: 14px;
  }
  .included :global(.glyph) {
    margin-top: 2px;
    color: var(--forest);
  }
  .subscribe .button {
    margin-top: 20px;
  }
  .plan-notes {
    border-top: 1px solid var(--line-2);
    padding-top: 20px;
    max-width: 900px;
    color: var(--muted);
    font-size: 13px;
    line-height: 1.7;
  }
  .plan-notes p {
    margin: 0 0 6px;
  }
  @media (max-width: 1000px) {
    .plans {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .plan:nth-child(odd) {
      padding-inline: 0 24px;
      border-inline-start: 0;
    }
    .plan:nth-child(even) {
      padding-inline: 24px 0;
    }
    .plan:nth-child(n + 3) {
      border-top: 1px solid var(--line-2);
    }
    .price bdi {
      font-size: 40px;
    }
    .membership-details {
      gap: 32px;
    }
  }
  @media (max-width: 560px) {
    .plans,
    .membership-details {
      grid-template-columns: minmax(0, 1fr);
    }
    .plan:nth-child(n) {
      padding: 28px 0;
      border-inline-start: 0;
    }
    .plan + .plan {
      border-top: 1px solid var(--line-2);
    }
    .plan-heading {
      grid-template-rows: auto;
    }
    .price {
      margin: 20px 0 24px;
    }
    .storage {
      flex-direction: row;
      padding-bottom: 12px;
    }
    .storage dd {
      font-size: 16px;
    }
    .membership-details {
      padding-block: 28px;
    }
  }
  :global(html[dir='rtl']) .plan h3,
  :global(html[dir='rtl']) .price bdi {
    letter-spacing: normal;
  }
</style>
