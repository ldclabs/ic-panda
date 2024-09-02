<script lang="ts">
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { MasterKey, type MyMessageState } from '$lib/stores/user'
  import { errMessage } from '$lib/types/result'
  import { AesGcmKey, randomBytes, utf8ToBytes } from '$lib/utils/crypto'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let masterKey: MasterKey | null

  const toastStore = getToastStore()
  const keyId = utf8ToBytes(String(Date.now()))

  let validating = false
  let submitting = false

  let passwordInput1 = ''
  let passwordInput2 = ''
  let passwordErr = ''
  let cachedPassword = masterKey ? masterKey.cachedPassword() : true
  let ecdhKeyPromise: Promise<AesGcmKey> | null = null

  function checkPassword() {
    passwordErr = ''
    if (passwordInput1.length < 6) {
      passwordErr = 'password must be at least 6 characters long'
    } else if (passwordInput1 !== passwordInput2) {
      passwordErr = 'password do not match'
    }
    return ''
  }

  async function onComfirm(_e: Event) {
    submitting = true

    try {
      if (masterKey) {
        try {
          await masterKey.open(
            passwordInput1,
            myState.id,
            cachedPassword ? 7 * 24 * 60 * 60 * 1000 : 0
          )
          parent && parent['onClose']()
        } catch (_err) {
          throw new Error('Incorrect password')
        }

        return
      }

      if (passwordInput1 != passwordInput2) {
        throw new Error('passwords do not match')
      }

      const kind = ecdhKeyPromise ? 'ECDH' : 'Local'
      const remoteSecret = ecdhKeyPromise
        ? (await ecdhKeyPromise).getSecretKey()
        : randomBytes(32)
      await myState.setMasterKey(
        kind,
        keyId,
        passwordInput1,
        remoteSecret,
        cachedPassword ? 7 * 24 * 60 * 60 * 1000 : 0
      )

      parent && parent['onClose']()
    } catch (err: any) {
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: true,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    ;(form['passwordInput2'] as HTMLInputElement)?.setCustomValidity(
      checkPassword()
    )
    validating = form.checkValidity()
  }

  onMount(async () => {
    if (!masterKey && myState.ecdhAvailable()) {
      ecdhKeyPromise = myState.loadECDHCoseEncryptedKey(keyId)
    }
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold"
    >{!masterKey ? 'Setup' : 'Enter'} Password</div
  >
  <form
    class="m-auto !mt-4 flex flex-col content-center"
    on:input|preventDefault|stopPropagation|stopImmediatePropagation={onFormChange}
  >
    {#if !masterKey}
      <div class="space-y-2 rounded-xl bg-gray/5 p-4">
        <p class="text-gray/50">
          <b>1.</b> The password is used only locally to derive the master key along
          with remote key material; neither the password nor the derived key is stored
          remotely.
        </p>
        <p class="text-gray/50">
          <b>2.</b> By default, the hashed password is cached in the browser database
          for 7 days.
        </p>
      </div>
    {/if}
    <input
      class="input mt-4 truncate rounded-xl border-gray/10 bg-white/20 invalid:input-warning hover:bg-white/90"
      type="password"
      name="passwordInput1"
      minlength="6"
      maxlength="32"
      bind:value={passwordInput1}
      disabled={submitting}
      placeholder="Enter password"
      required
    />
    {#if !masterKey}
      <input
        class="input mt-4 truncate rounded-xl border-gray/10 bg-white/20 invalid:input-warning hover:bg-white/90"
        type="password"
        name="passwordInput2"
        minlength="6"
        maxlength="32"
        bind:value={passwordInput2}
        disabled={submitting}
        placeholder="Enter password again"
        required
      />
      <p
        class="h-5 pl-2 text-sm text-error-500 {passwordErr == ''
          ? 'invisible'
          : 'visiable'}">{passwordErr}</p
      >
    {/if}
    <label class="mt-2 flex items-center">
      <input class="checkbox" type="checkbox" bind:checked={cachedPassword} />
      <p class="ml-2 text-sm text-gray/50">Cache password for 7 days</p>
    </label>
  </form>
  <footer class="m-auto !mt-4">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating}
      on:click={onComfirm}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>Comfirm</span>
      {/if}
    </button>
  </footer>
</ModalCard>
