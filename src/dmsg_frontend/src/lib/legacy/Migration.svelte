<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import {
    collectArchive,
    encodeArchive,
    pack,
    pairingFingerprint,
    readOfferLink,
    sealTransfer,
    type Inventory,
    type PairingOffer
  } from '@dmsg/legacy'
  import { createLegacyReader } from './reader'
  import { authStore } from '$lib/stores/auth'
  import { t } from '$lib/i18n'
  import Check from './ui/Check.svelte'
  import Choice from './ui/Choice.svelte'
  import Step from './ui/Step.svelte'
  import { chip, field, panel } from './ui/classes'
  const modes = ['Local', 'ECDH', 'VetKey'] as const
  let mode = $state<(typeof modes)[number]>('Local')
  let password = $state(''),
    offer = $state<PairingOffer | null>(null),
    linkError = $state(''),
    confirmed = $state(false)
  let report = $state<Inventory | null>(null),
    error = $state(''),
    busy = $state(false)
  let objectCount = $state(0)
  let abort: AbortController | null = null
  // The desktop client opens this page with its pairing offer in the URL
  // fragment, which never reaches a server; drop it from the address bar.
  // Errors and states hold English message keys and render through $t.
  onMount(() => {
    if (!location.hash.startsWith('#pair=')) return
    try {
      offer = readOfferLink(location.hash, location.origin)
    } catch {
      linkError =
        'This pairing link is invalid, expired or meant for another site. Create a new one in the dMsg desktop app.'
    }
    history.replaceState(null, '', location.pathname + location.search)
  })
  onDestroy(() => {
    abort?.abort()
    password = ''
    report = null
  })
  const anonymous = $derived($authStore.identity.getPrincipal().isAnonymous())

  async function clearCache() {
    const { checkpoint } = await createLegacyReader(mode)
    try {
      await checkpoint.clear()
      report = null
      objectCount = 0
    } finally {
      checkpoint.close()
    }
  }

  async function run(exporting: boolean) {
    if (busy) return
    busy = true
    error = ''
    abort = new AbortController()
    try {
      const { reader, checkpoint } = await createLegacyReader(mode)
      try {
        if (exporting) {
          if (!offer || !confirmed)
            throw new Error(
              'First compare the pairing fingerprint and the target device ID on both sides.'
            )
          if (offer.expiresAt <= Date.now())
            throw new Error(
              'The pairing has expired. Create a new one in the desktop app.'
            )
          const archive = await collectArchive(
            reader,
            {
              password,
              salt: $authStore.identity.getPrincipal().toText(),
              keyId: pack('PANDA'),
              contextVersion: 1,
              keyName: mode === 'VetKey' ? 'key_1' : 'test_key_1'
            },
            abort.signal
          )
          const encoded = encodeArchive(archive)
          try {
            const encrypted = await sealTransfer(encoded, offer, {
              origin: location.origin,
              target: offer.target,
              fingerprint: pairingFingerprint(offer)
            })
            const url = URL.createObjectURL(
              new Blob([encrypted], { type: 'application/octet-stream' })
            )
            const a = document.createElement('a')
            a.href = url
            a.download = 'legacy.dmsg-migration'
            a.click()
            setTimeout(() => URL.revokeObjectURL(url), 30000)
            // Do not retain secret-bearing source objects in a reactive UI tree.
            objectCount = reader.inventory.objects.length
            report = {
              ...reader.inventory,
              objects: [],
              calls: [],
              gaps: reader.inventory.gaps
            }
          } finally {
            encoded.fill(0)
            archive.keys.forEach((k) => k.coseKey.fill(0))
          }
        } else {
          report = await reader.collect(abort.signal)
          objectCount = report.objects.length
        }
      } finally {
        checkpoint.close()
      }
    } catch (cause) {
      error =
        cause instanceof Error ? cause.message : 'The inventory did not finish.'
    } finally {
      password = ''
      busy = false
      abort = null
    }
  }
</script>

<section
  class="migration grid max-w-[860px] gap-6 border-t border-[var(--line)] py-12"
>
  <div class="grid gap-3">
    <h2>{$t('Read-only inventory and encrypted migration')}</h2>
    <p class="mb-0!">
      {$t(
        'Use your legacy identity and choose the key mode you used before. Reading never initializes or rotates keys, confirms exchanges, or marks anything read. The result is a pre-migration snapshot and cannot confirm the final cutover.'
      )}
    </p>
  </div>
  <ol class={panel}>
    <Step n={1} title={$t('Your legacy identity')} done={!anonymous}>
      <div class="flex flex-wrap items-center gap-2 text-[15px]">
        <span class="text-[var(--muted)]">{$t('Current identity:')}</span>
        <code class={chip}>{$authStore.identity.getPrincipal().toText()}</code>
      </div>
      <div class="grid gap-2">
        <span class="text-[14px] font-medium">{$t('Legacy key mode')}</span>
        <Choice
          bind:value={() => mode, (next) => (mode = next as typeof mode)}
          options={[...modes]}
          label={$t('Legacy key mode')}
          disabled={busy}
        />
      </div>
    </Step>
    <Step n={2} title={$t('Take an inventory')} done={!!report}>
      <div class="flex flex-wrap items-center gap-3">
        <button
          class="button secondary disabled:cursor-not-allowed disabled:opacity-60"
          disabled={busy || anonymous}
          onclick={() => run(false)}
          >{$t('Take inventory of readable history')}</button
        >
        <button
          class="text-link disabled:cursor-not-allowed disabled:opacity-60"
          disabled={busy}
          onclick={clearCache}
          >{$t('Clear the migration cache and start over')}</button
        >
      </div>
      {#if report}<div
          class="grid gap-2 rounded-xl bg-[var(--canvas)] p-4"
          role="status"
        >
          <span class="text-[15px] font-medium"
            >{$t(
              'Read {count} records with {gaps} gaps. Query results are not a certified frozen snapshot.',
              { count: objectCount, gaps: report.gaps.length }
            )}</span
          >
          {#if report.gaps.length}<ul
              class="grid max-h-60 gap-1 overflow-y-auto font-mono text-[12.5px] text-[var(--muted)]"
            >
              {#each report.gaps.slice(0, 30) as gap}<li>
                  <code class="text-[var(--ink)]">{gap.source}</code>: {gap.code}
                  · {gap.detail}
                </li>{/each}
            </ul>{/if}
        </div>{/if}
      <p class="mb-0! text-[14px]">
        {$t(
          'Pages already read are kept encrypted in a separate migration cache in this browser, so you can continue after reopening. The original database stays read-only, and changes in the old service are reported as differences.'
        )}
      </p>
    </Step>
    <Step n={3} title={$t('Compare with the desktop app')} done={confirmed}>
      {#if offer}<dl
          class="grid gap-x-6 gap-y-2 rounded-xl border border-[var(--accent)] bg-[var(--pale)] p-4 sm:grid-cols-[max-content_minmax(0,1fr)]"
        >
          <dt class="text-[14px] text-[var(--muted)]">{$t('Target device:')}</dt
          >
          <dd class="m-0 font-mono text-[14px] [overflow-wrap:anywhere]">
            {offer.target}
          </dd>
          <dt class="text-[14px] text-[var(--muted)]">{$t('Fingerprint:')}</dt>
          <dd
            class="m-0 font-mono text-[15px] font-semibold [overflow-wrap:anywhere] text-[var(--action)]"
          >
            {pairingFingerprint(offer)}
          </dd>
        </dl>
        <Check bind:checked={confirmed}
          >{$t(
            'The fingerprint, legacy identity and target device match what the desktop app shows'
          )}</Check
        >{:else}<p
          class={[
            'mb-0! rounded-xl p-4 text-[15px]',
            linkError ? 'bg-[#fef2f2] text-[#991b1b]!' : 'bg-[var(--canvas)]'
          ]}
          role={linkError ? 'alert' : undefined}
        >
          {$t(
            linkError ||
              'Create a pairing in the Legacy section of the dMsg desktop app; it opens a link with the pairing in this browser.'
          )}
        </p>{/if}
    </Step>
    <Step n={4} title={$t('Create the migration file')}>
      {#if mode !== 'VetKey'}<label class="grid max-w-md gap-2">
          <span class="text-[14px] font-medium"
            >{$t('Your original unlock password')}</span
          >
          <input
            type="password"
            class={field}
            bind:value={password}
            autocomplete="off"
          />
        </label>{/if}
      <div class="flex flex-wrap items-center gap-3">
        <button
          class="button primary disabled:cursor-not-allowed disabled:opacity-60"
          disabled={busy || !offer || !confirmed || anonymous}
          onclick={() => run(true)}
          >{$t('Unlock and create the encrypted migration file')}</button
        >
        {#if busy}<button
            class="button secondary disabled:cursor-not-allowed disabled:opacity-60"
            onclick={() => abort?.abort()}>{$t('Stop reading')}</button
          >{/if}
      </div>
      {#if error}<p
          class="mb-0! rounded-xl bg-[#fef2f2] p-4 text-[15px] text-[#991b1b]!"
          role="alert"
        >
          {$t(error)}
        </p>{/if}
      <p class="mb-0! text-[14px]">
        {$t(
          'Only the desktop device of this pairing can decrypt the file, and the pairing is valid for ten minutes. Keep the original browser data and the old service until independent recovery has been verified. Missing keys, permissions and unfinished uploads are listed separately.'
        )}
      </p>
    </Step>
  </ol>
</section>
