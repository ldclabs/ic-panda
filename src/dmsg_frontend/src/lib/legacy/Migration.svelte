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
  let mode = $state<'Local' | 'ECDH' | 'VetKey'>('Local')
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

<section class="migration">
  <h2>{$t('Read-only inventory and encrypted migration')}</h2>
  <p>
    {$t(
      'Use your legacy identity and choose the key mode you used before. Reading never initializes or rotates keys, confirms exchanges, or marks anything read. The result is a pre-migration snapshot and cannot confirm the final cutover.'
    )}
  </p>
  <p>
    {$t('Current identity:')}
    <code>{$authStore.identity.getPrincipal().toText()}</code>
  </p>
  <label
    >{$t('Legacy key mode')}
    <select bind:value={mode} disabled={busy}
      ><option>Local</option><option>ECDH</option><option>VetKey</option
      ></select
    ></label
  >
  <button
    class="button"
    disabled={busy || $authStore.identity.getPrincipal().isAnonymous()}
    onclick={() => run(false)}
    >{$t('Take inventory of readable history')}</button
  >
  {#if report}<p role="status">
      {$t(
        'Read {count} records with {gaps} gaps. Query results are not a certified frozen snapshot.',
        { count: objectCount, gaps: report.gaps.length }
      )}
    </p>
    <ul>
      {#each report.gaps.slice(0, 30) as gap}<li>
          <code>{gap.source}</code>: {gap.code} · {gap.detail}
        </li>{/each}
    </ul>
  {/if}
  <p>
    {$t(
      'Pages already read are kept encrypted in a separate migration cache in this browser, so you can continue after reopening. The original database stays read-only, and changes in the old service are reported as differences.'
    )}
  </p>
  <button
    class="button"
    disabled={busy}
    onclick={async () => {
      const { checkpoint } = await createLegacyReader(mode)
      try {
        await checkpoint.clear()
        report = null
        objectCount = 0
      } finally {
        checkpoint.close()
      }
    }}>{$t('Clear the migration cache and start over')}</button
  >
  {#if offer}<p>{$t('Target device:')} <code>{offer.target}</code></p>
    <p>{$t('Fingerprint:')} <code>{pairingFingerprint(offer)}</code></p>
    <label
      ><input type="checkbox" bind:checked={confirmed} />
      {$t(
        'The fingerprint, legacy identity and target device match what the desktop app shows'
      )}</label
    >{:else}<p
      >{$t(
        linkError ||
          'Create a pairing in the Legacy section of the dMsg desktop app; it opens a link with the pairing in this browser.'
      )}</p
    >{/if}
  {#if mode !== 'VetKey'}<label
      >{$t('Your original unlock password')}<input
        type="password"
        bind:value={password}
        autocomplete="off"
      /></label
    >{/if}
  <button
    class="button primary"
    disabled={busy ||
      !offer ||
      !confirmed ||
      $authStore.identity.getPrincipal().isAnonymous()}
    onclick={() => run(true)}
    >{$t('Unlock and create the encrypted migration file')}</button
  >
  {#if busy}<button class="button" onclick={() => abort?.abort()}
      >{$t('Stop reading')}</button
    >{/if}
  {#if error}<p role="alert">{$t(error)}</p>{/if}
  <p>
    {$t(
      'Only the desktop device of this pairing can decrypt the file, and the pairing is valid for ten minutes. Keep the original browser data and the old service until independent recovery has been verified. Missing keys, permissions and unfinished uploads are listed separately.'
    )}
  </p>
</section>

<style>
  .migration {
    padding: 32px 0;
    border-top: 1px solid #cdd8cf;
    max-width: 780px;
  }
  label {
    display: block;
    margin: 16px 0;
  }
  code {
    overflow-wrap: anywhere;
  }
  button {
    margin: 8px 12px 8px 0;
  }
</style>
