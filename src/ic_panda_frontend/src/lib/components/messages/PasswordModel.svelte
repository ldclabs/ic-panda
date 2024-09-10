<script lang="ts">
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { toastRun } from '$lib/stores/toast'
  import { encodeCBOR, randomBytes } from '$lib/utils/crypto'
  import { MasterKey, type MyMessageState } from '$src/lib/stores/message'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let masterKey: MasterKey | null

  const toastStore = getToastStore()
  const keyId = encodeCBOR('ICPanda_Messages_Master_Key')
  const PasswordExpire = 7 * 24 * 60 * 60 * 1000

  let validating = false
  let submitting = false

  let passwordInput1 = ''
  let passwordInput2 = ''
  let passwordErr = ''
  let processingTip = 'Derive keys...'
  let cachedPassword = masterKey ? masterKey.cachedPassword() : true

  function checkPassword() {
    passwordErr = ''
    if (passwordInput1.length < 6) {
      passwordErr = 'password must be at least 6 characters long'
    } else if (passwordInput1 !== passwordInput2) {
      passwordErr = 'password do not match'
    }
    return ''
  }

  function onComfirm() {
    submitting = true

    toastRun(async () => {
      const kind = myState.masterKeyKind()

      if (masterKey) {
        try {
          await masterKey.open(
            passwordInput1,
            myState.id,
            cachedPassword ? PasswordExpire : 0
          )

          if (masterKey.kind !== kind) {
            processingTip = 'Migrate keys...'
            await myState.migrateKeys()
          } else {
            await myState.initStaticECDHKey()
          }

          parent && parent['onClose']()
        } catch (err) {
          throw new Error('Incorrect password')
        }

        return
      }

      if (passwordInput1 != passwordInput2) {
        throw new Error('passwords do not match')
      }

      let remoteSecret: Uint8Array
      switch (kind) {
        case 'Local':
          remoteSecret = randomBytes(32)
          break
        case 'ECDH':
          const remoteKey = await myState.fetchECDHCoseEncryptedKey(keyId)
          remoteSecret = remoteKey.getSecretKey()
          break
        default:
          throw new Error('Invalid master key kind')
      }

      await myState.setMasterKey(
        kind,
        keyId,
        passwordInput1,
        remoteSecret,
        cachedPassword ? PasswordExpire : 0
      )

      await myState.initStaticECDHKey()
      parent && parent['onClose']()
    }, toastStore).finally(() => {
      submitting = false
      validating = false
    })
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

  onMount(() => {})
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
          <b>2.</b> By default, the hashed password is cached in the browser storage
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
        <span>{processingTip}</span>
      {:else}
        <span>Comfirm</span>
      {/if}
    </button>
  </footer>
</ModalCard>
