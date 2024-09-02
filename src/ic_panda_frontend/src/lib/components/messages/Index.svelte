<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { myMessageStateAsync, type MyMessageState } from '$lib/stores/user'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import Home from './Home.svelte'
  import PasswordModel from './PasswordModel.svelte'

  const modalStore = getModalStore()

  let myState: MyMessageState
  let myInfo: Readable<UserInfo>

  onMount(async () => {
    myState = await myMessageStateAsync()
    myInfo = myState.info
    console.log('myInfo', $myInfo)
    const mk = await myState.masterKey()
    if (!myState.principal.isAnonymous() && (!mk || !mk.isOpened())) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: PasswordModel,
          props: {
            myState: myState,
            myInfo: myInfo,
            masterKey: mk
          }
        }
      })
    }
  })
</script>

{#if $myInfo}
  <div class="mt-4 w-full max-w-4xl">
    <div class="card rounded-2xl bg-white">
      <section class="p-6">
        <p class="min-w-0 text-pretty break-words max-sm:max-w-52"
          >{$myInfo.name}</p
        >
      </section>
    </div>
  </div>
{:else}
  <Home />
{/if}
