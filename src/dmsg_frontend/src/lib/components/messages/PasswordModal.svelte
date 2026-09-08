<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { type MasterKey, type MyMessageState } from '$lib/stores/message'
  import { onMount, type SvelteComponent } from 'svelte'

  let {
    parent,
    myState,
    masterKey,
    iv,
    onCompleted
  }: {
    parent: SvelteComponent
    myState: MyMessageState
    masterKey: MasterKey | null
    iv: Uint8Array
    onCompleted: () => void
  } = $props()
  let password = $state('')
  let busy = $state(false)
  let checking = $state(true)
  let recoverable = $state(false)
  let error = $state('')

  onMount(async () => {
    try {
      // Only reconstruct an old ECDH key when the old password verifier exists.
      // Missing Local key material must not be replaced by newly generated keys.
      recoverable =
        !!masterKey ||
        (myState.agent.hasCOSE && !!(await myState.getPasswordHash()))
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Could not read recovery information.'
    } finally {
      checking = false
    }
  })

  async function unlock(event: SubmitEvent) {
    event.preventDefault()
    if (busy || checking || !recoverable) return
    busy = true
    error = ''
    try {
      if (masterKey) {
        await masterKey.open(password, myState.id, 0, iv)
      } else {
        const remoteKey = await myState.fetchECDHCoseEncryptedKey()
        await myState.setMasterKey(
          'ECDH',
          password,
          remoteKey.getSecretKey(),
          0,
          iv
        )
      }
      // Cache the existing key locally; never migrate keys or initialize ECDH.
      await myState.saveMasterKeys()
      password = ''
      parent['onClose']()
      onCompleted()
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Could not unlock your existing key.'
    } finally {
      busy = false
    }
  }
</script>

<ModalCard {parent} cardClass="!bg-white !text-[#10251f]">
  <h2 class="text-xl font-semibold">Unlock legacy history</h2>
  <p class="mt-3 text-sm text-[#4e6257]"
    >Use your existing password. This only unlocks historical content; it does
    not upgrade, reset, or replace your keys.</p
  >
  {#if checking}<p class="mt-4" role="status"
      >Checking existing recovery materials…</p
    >
  {:else if recoverable}
    <form class="mt-5 flex flex-col gap-4" onsubmit={unlock}>
      <label for="legacy-password">Existing password</label>
      <input
        id="legacy-password"
        class="input rounded-lg border border-[#607566] bg-white"
        type="password"
        autocomplete="current-password"
        bind:value={password}
        required
        disabled={busy}
      />
      <button
        class="btn rounded-lg bg-[#145c45] px-5 py-3 text-white"
        type="submit"
        disabled={busy || !password}
        >{busy ? 'Unlocking…' : 'Unlock history'}</button
      >
    </form>
  {:else}<p class="mt-4 rounded-lg bg-[#edf1ea] p-4 text-sm"
      >Existing key material was not found. Return to the original browser and
      account, or use your existing recovery materials. Local keys cannot be
      recreated by resetting a password.</p
    >{/if}
  {#if error}<p class="mt-4 text-sm text-red-800" role="alert">{error}</p>{/if}
</ModalCard>
