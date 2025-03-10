<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconSubtractLine from '$lib/components/icons/IconSubtractLine.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import {
    toDisplayUserInfo,
    type DisplayUserInfo,
    type MyMessageState
  } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { unwrapOption } from '$lib/types/result'
  import { clickOutside } from '$lib/utils/window'
  import type { ProfileInfo } from '$src/declarations/ic_message_profile/ic_message_profile.did'
  import { Principal } from '@dfinity/principal'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, type SvelteComponent } from 'svelte'
  import { writable, type Writable } from 'svelte/store'

  type MemberInfoEx = DisplayUserInfo & {
    isManager: boolean
    isMember: boolean
    ecdhPub: Uint8Array | null
  }

  interface Props {
    parent: SvelteComponent
    isAddManager?: boolean
    existsManagers?: string[]
    existsMembers?: string[]
    myState: MyMessageState
    onSave: (users: [Principal, Uint8Array | null][]) => Promise<void>
  }

  let {
    parent,
    isAddManager = false,
    existsManagers = [],
    existsMembers = [],
    myState,
    onSave
  }: Props = $props()

  const title: string = isAddManager ? 'Add Managers' : 'Add members'
  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const selectedUsers: Writable<MemberInfoEx[]> = writable([])
  const searchUsers: Writable<MemberInfoEx[]> = writable([])

  function toMemberInfoEx(user: UserInfo & ProfileInfo): MemberInfoEx {
    const info = toDisplayUserInfo(user) as MemberInfoEx
    info.isManager = existsManagers.includes(info._id)
    info.isMember = existsMembers.includes(info._id)
    info.ecdhPub = unwrapOption(user.ecdh_pub as [] | [Uint8Array])
    return info
  }

  let elemSearcher: HTMLElement | undefined = $state()
  let userInput = $state('')
  let submitting = $state(false)

  const debouncedSearch = debounce(async () => {
    const name = userInput.trim()
    let id: Principal | null = null
    if (name.length > 60) {
      try {
        id = Principal.fromText(name)
      } catch (e) {}
    }

    try {
      if (id) {
        let user = await myState.tryLoadProfile(id)
        if (!user) {
          user = { id: id, name: name, username: [] } as UserInfo & ProfileInfo
        }
        searchUsers.set([toMemberInfoEx(user)])
      } else {
        searchUsers.set([])
        const names = await myState.api.search_username(name)

        await Promise.all(
          names.map(async (name) => {
            const userProfile = await myState.tryLoadProfile(name)
            if (userProfile) {
              const info = toMemberInfoEx(userProfile)
              searchUsers.update((prev) => {
                if (!prev.some((u) => u._id === info._id)) {
                  return [...prev, info]
                }
                return prev
              })
            }
          })
        )
      }
    } catch (err) {
      console.error(err)
      // ignore
    }
  }, 618)

  function onSearchUsername() {
    debouncedSearch()
  }

  function onSelectUser(user: MemberInfoEx) {
    searchUsers.update((prev) => {
      return prev.filter((u) => u._id !== user._id)
    })

    selectedUsers.update((prev) => {
      if (prev.some((u) => u._id === user._id)) {
        return [...prev]
      }
      return [...prev, user]
    })
  }

  function onUnSelectUser(user: MemberInfoEx) {
    selectedUsers.update((prev) => {
      return prev.filter((u) => u._id !== user._id)
    })
  }

  async function onSaveHandler() {
    submitting = true
    toastRun(async (signal: AbortSignal) => {
      const users: [Principal, Uint8Array | null][] = []
      for (const info of $selectedUsers) {
        if (!info.src!.id) {
          continue
        }

        if (info.isManager) {
          continue
        }

        if (!isAddManager && (info.isManager || info.isMember)) {
          continue
        }

        if (!info.ecdhPub) {
          // try to fetch the latest ecdh public key
          const ninfo = await myState.tryFetchProfile(info.src!.id)
          if (ninfo) {
            info.ecdhPub = unwrapOption(ninfo.ecdh_pub as [] | [Uint8Array])
          }
        }
        users.push([info.src!.id, info.ecdhPub])
      }

      if (users.length > 0) {
        await onSave(users)
      }

      modalStore.close()
    }, toastStore).finally(() => {
      submitting = false
    })
  }

  onMount(() => {
    if (!elemSearcher) {
      return
    }

    const aborter = clickOutside(elemSearcher, () => {
      searchUsers.set([])
    })

    return () => {
      aborter()
    }
  })

  onDestroy(() => {
    debouncedSearch.clear()
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">{title}</div>
  <section class="relative m-auto !mt-4 flex flex-col content-center">
    <input
      class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
      type="text"
      name="userInput"
      minlength="1"
      maxlength="64"
      data-1p-ignore
      bind:value={userInput}
      oninput={onSearchUsername}
      placeholder="username or principal"
    />
    <div
      class="card mt-4 max-h-96 min-h-48 w-full space-y-1 overflow-y-auto bg-surface-500/5 py-2"
    >
      {#each $selectedUsers as user (user._id)}
        <div
          class="grid w-full grid-cols-[1fr_auto] items-center rounded-none p-2"
        >
          <div class="flex flex-row items-center space-x-2">
            <Avatar
              initials={user.name}
              src={user.image}
              fill="fill-white"
              width="w-10"
            />
            <p class="ml-1 max-w-52 truncate text-ellipsis">
              {user.name + (user.username ? ' @' + user.username : '')}
            </p>
          </div>
          <button
            class="pointer btn btn-sm hover:bg-panda/10"
            onclick={() => onUnSelectUser(user)}
          >
            <span class="text-neutral-300 *:size-6"><IconSubtractLine /></span>
          </button>
        </div>
      {/each}
    </div>

    <div
      bind:this={elemSearcher}
      class="card absolute left-0 top-10 mt-4 max-h-48 w-full space-y-1 overflow-y-auto bg-white py-2 shadow-xl {$searchUsers.reduce(
        (acc, val) =>
          val.isManager || (val.isMember && !isAddManager) ? acc : acc + 1,
        0
      ) === 0
        ? 'hidden'
        : ''}"
    >
      {#each $searchUsers as user (user._id)}
        <button
          class="pointer btn grid w-full grid-cols-[1fr_auto] items-center rounded-none p-2 hover:bg-panda/10"
          onclick={() => onSelectUser(user)}
          disabled={user.isManager || (user.isMember && !isAddManager)}
        >
          <div class="flex flex-row items-center space-x-2">
            <Avatar
              initials={user.name}
              src={user.image}
              fill="fill-white"
              width="w-10"
            />
            <p class="ml-1 max-w-52 truncate text-ellipsis">
              {user.name + (user.username ? ' @' + user.username : '')}
            </p>
          </div>
        </button>
      {/each}
    </div>
  </section>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || $selectedUsers.length === 0}
      onclick={onSaveHandler}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>Confirm</span>
      {/if}
    </button>
  </footer>
</ModalCard>
