<script lang="ts">
  import { locale, t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import { reveal } from './motion'
  import membership from './membership.json'

  // Public marketing snapshot, checked 2026-10-28: effective commerce catalog
  // v2 (uwist-sqaaa-aaaaj-qsjyq-cai), wallet hub get_hub_config
  // (ghcje-dyaaa-aaaas-qg7va-cai), and dmsg_types::billing::PlanId::device_limit.
  // Backup file limits follow the public cloud contract (2026-11-02).
  // Recheck both services when updating their figures; Rust's default catalog
  // is not the live catalog. Checkout always confirms current terms in the app.
  const { plans } = membership

  const number = $derived(new Intl.NumberFormat($locale))
  const price = $derived(
    new Intl.NumberFormat($locale, {
      style: 'currency',
      currency: 'USD',
      currencyDisplay: 'narrowSymbol',
      maximumFractionDigits: 0
    })
  )
  type Plan = (typeof plans)[number]
  const rows = $derived<{ label: string; value: (plan: Plan) => string }[]>([
    {
      label: 'Encrypted cloud storage',
      value: (plan) => `${number.format(plan.storage)} ${plan.unit}`
    },
    {
      label: 'Cloud backup file limit',
      value: (plan) => `${number.format(plan.backupFileMiB)} MiB`
    },
    { label: 'Active devices', value: (plan) => number.format(plan.devices) },
    { label: 'Active channels', value: (plan) => number.format(plan.channels) },
    {
      label: 'Formal signatures / month',
      value: (plan) => number.format(plan.signatures)
    },
    {
      label: 'Multisig wallet slots',
      value: (plan) => number.format(plan.wallets)
    }
  ])
  const checkedAt = $derived(
    new Intl.DateTimeFormat($locale, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      timeZone: 'UTC'
    }).format(new Date(`${membership.checkedAt}T00:00:00Z`))
  )
</script>

<section
  class="section membership band"
  id="membership"
  aria-labelledby="membership-title"
>
  <div class="wrap">
    <header class="sec-head" use:reveal>
      <p class="kicker"><b>06</b>{$t('Plans')}</p>
      <h2 id="membership-title"
        >{$t('Start free.')}<br /><span class="soft"
          >{$t('Add room when you need it.')}</span
        ></h2
      >
      <p class="sec-lead"
        >{$t(
          'Choose the storage and device capacity that fits your everyday work. Upgrade to collect files by upload link and make room for more.'
        )}</p
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
            <div class="backup-limit"
              ><dt>{$t('Cloud backup file limit')}</dt><dd
                ><bdi>{number.format(plan.backupFileMiB)} MiB</bdi></dd
              ></div
            >
          </dl>
          <p class="plan-feature"
            >{#if plan.fileRequests}<Glyph name="check" size={16} />{$t(
                'File requests included'
              )}{/if}</p
          >
        </article>
      {/each}
    </div>

    <details class="compare">
      <summary
        >{$t('Compare plan limits')}<Glyph
          name="arrow-right"
          size={18}
        /></summary
      >
      <!-- Focusable so the keyboard can scroll the table where it is wider than the screen. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div
        class="compare-table"
        tabindex="0"
        role="region"
        aria-label={$t('Compare plan limits')}
      >
        <table>
          <thead>
            <tr
              ><td></td>{#each plans as plan}<th scope="col"
                  ><bdi>{plan.name}</bdi></th
                >{/each}</tr
            >
          </thead>
          <tbody>
            {#each rows as row}
              <tr
                ><th scope="row">{$t(row.label)}</th>{#each plans as plan}<td
                    ><bdi>{row.value(plan)}</bdi></td
                  >{/each}</tr
              >
            {/each}
          </tbody>
        </table>
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
            'The backup file limit applies to cloud backups. File sharing and channel attachments have their own limits.'
          )}</p
        >
      </div>
    </details>

    <div class="membership-details" use:reveal>
      <div class="included">
        <Glyph name="lock" size={22} />
        <div
          ><h3>{$t('Privacy comes with every plan.')}</h3><p
            >{$t(
              'Every plan includes encryption and device revocation. Recovery settings are available across plans; review the setup requirements before relying on recovery.'
            )}</p
          ></div
        >
      </div>
      <div class="subscribe">
        <h3>{$t('Choose your plan in dMsg.')}</h3>
        <p
          >{$t(
            'Review the current price, payment options and renewal terms in the app before you confirm.'
          )}</p
        >
        <a class="button primary" href="#release"
          >{$t('Download dMsg')}<Glyph name="arrow-right" /></a
        >
      </div>
    </div>
    <div class="plan-notes">
      <p
        >{$t('Prices checked on {date}. Confirm current terms in the app.', {
          date: checkedAt
        })}</p
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
    grid-row: span 5;
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
    grid-row: span 3;
    grid-template-rows: subgrid;
    margin: 0;
  }
  .plan-feature {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 24px;
    margin: 16px 0 0;
    color: var(--forest);
    font-size: 14px;
    font-weight: 600;
  }
  .compare {
    border-bottom: 1px solid var(--line-2);
  }
  .compare > summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 64px;
    padding: 16px 2px;
    cursor: pointer;
    font-size: 16px;
    font-weight: 550;
    list-style: none;
  }
  .compare > summary::-webkit-details-marker {
    display: none;
  }
  .compare > summary :global(.glyph) {
    transition: transform 0.3s var(--ease);
  }
  .compare[open] > summary :global(.glyph) {
    transform: rotate(90deg);
  }
  .compare-table {
    overflow-x: auto;
  }
  table {
    width: 100%;
    min-width: 560px;
    border-collapse: collapse;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }
  thead th {
    padding: 10px 12px;
    font-size: 15px;
    font-weight: 650;
    text-align: end;
  }
  tbody th {
    padding: 12px 12px 12px 0;
    color: var(--muted);
    font-weight: 400;
    text-align: start;
  }
  :global(html[dir='rtl']) tbody th {
    padding: 12px 0 12px 12px;
  }
  tbody td {
    padding: 12px;
    font-weight: 600;
    text-align: end;
    white-space: nowrap;
  }
  tbody tr {
    border-top: 1px solid var(--line);
  }
  .compare .plan-notes {
    margin: 12px 0 24px;
    border-top: 0;
    padding-top: 0;
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
    .plan-feature:empty {
      display: none;
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
