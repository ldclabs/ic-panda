<script lang="ts">
  import { onMount } from 'svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import Confirm from '../../components/Confirm.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { CallError, call, when } from '../../rpc'
  import { walletError as errorText } from './errors'
  import type { Wallet } from './model'
  import { w } from './text'

  /**
   * This device's native Taproot key root: how it is protected, the hardware
   * keys that open it, and this wallet's key derived from it. Without a
   * Secure Enclave the person explicitly chooses a root kept in software.
   */
  let { wallet }: { wallet: Wallet } = $props()

  let native = $state<any>(null),
    biometryOnly = $state(false),
    software = $state(false),
    busy = $state(false),
    error = $state('')
  let alive = true
  const fingerprints = $derived(native?.hardware === 'biometrics')

  async function load() {
    try {
      const value = await call('wallet/native_status', { wallet: wallet.wallet })
      if (alive) {
        native = value
        error = ''
      }
    } catch (e) {
      if (alive) error = errorText(e)
    }
  }

  async function run(method: string, params: Record<string, unknown> = {}) {
    if (busy) return
    busy = true
    error = ''
    try {
      await call(method, params)
      if (alive) await load()
    } catch (e) {
      if (!alive) return
      if (method === 'wallet/native_create' && !params.software && e instanceof CallError && e.code === 'UNAVAILABLE') software = true
      else error = errorText(e)
    } finally {
      busy = false
    }
  }

  onMount(() => {
    void load()
    return () => {
      alive = false
    }
  })
</script>

<Section title={w('nativeKeys')} description={w('nativeKeysHint')} icon="key" testid="wallet-native">
  <div class="grid gap-3">
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    {#if !native}
      <p class="text-muted text-[12px]" role="status">{w(error ? 'retry' : 'loading')}</p>
    {:else if !native.created}
      {#if fingerprints}<Checkbox bind:checked={biometryOnly} disabled={busy}>{w('nativeBiometryOnly')}</Checkbox>{/if}
      <div
        ><button
          class={buttonClass('primary', 'sm')}
          disabled={busy}
          aria-busy={busy}
          onclick={() => run('wallet/native_create', { software: false, biometry_only: fingerprints && biometryOnly })}
          data-testid="wallet-native-create">{w('nativeCreate')}</button
        ></div
      >
    {:else}
      <div class="flex flex-wrap items-center gap-2"
        ><Pill tone={native.protection === 'hardware' ? 'forest' : 'warn'}
          >{w(native.protection === 'hardware' ? 'nativeHardware' : 'nativeSoftware')}</Pill
        ><span class="text-faint text-[12px]">{when(native.created_at)}</span></div
      >
      {#if native.protection === 'hardware'}
        <ul class="divide-line grid divide-y" aria-label={w('nativeProtectors')}>
          {#each native.protectors as p (p.index)}<li class="flex flex-wrap items-center gap-2 py-2"
              ><b class="text-[13px]">{t('wallet.nativeProtector', { n: p.index + 1 })}</b>{#if p.biometry_only}<Pill
                  >{w('nativeBiometryOnly')}</Pill
                >{/if}<span class="text-faint min-w-0 flex-1 text-[12px]">{when(p.created_at)}</span
              ><button
                class={buttonClass('ghost', 'xs')}
                disabled={busy || native.protectors.length < 2}
                onclick={() => run('wallet/native_protector_remove', { index: p.index })}>{w('remove')}</button
              ></li
            >{/each}
        </ul>
        <div class="flex flex-wrap items-center gap-3"
          >{#if fingerprints}<Checkbox bind:checked={biometryOnly} disabled={busy}>{w('nativeBiometryOnly')}</Checkbox>{/if}<button
            class={buttonClass('secondary', 'sm')}
            disabled={busy}
            aria-busy={busy}
            onclick={() => run('wallet/native_protector_add', { biometry_only: fingerprints && biometryOnly })}
            data-testid="wallet-native-protect">{w('nativeProtectorAdd')}</button
          ></div
        >
      {:else}<Notice tone="warn">{w('nativeSoftwareBody')}</Notice>{/if}
      <Collapsible
        >{#snippet title()}<span class="text-[12px]">{w('nativeKey')}</span>{/snippet}<p class="font-mono text-[12px] break-all" dir="ltr"
          >{native.key?.public_key ?? w('nativeKeyNone')}</p
        ></Collapsible
      >
    {/if}
  </div>
</Section>
<Confirm
  bind:open={software}
  title={w('nativeSoftwareTitle')}
  description={w('nativeSoftwareBody')}
  confirm={w('nativeSoftwareCreate')}
  icon="key"
  tone="warn"
  {busy}
  onConfirm={() => run('wallet/native_create', { software: true, biometry_only: false })}
  testid="wallet-native-software"
/>
