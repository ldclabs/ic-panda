<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import Registration from '$lib/components/landing/Registration.svelte'
  import SectionArchive from '$lib/components/landing/SectionArchive.svelte'
  import SectionCta from '$lib/components/landing/SectionCta.svelte'
  import SectionDao from '$lib/components/landing/SectionDao.svelte'
  import SectionHero from '$lib/components/landing/SectionHero.svelte'
  import SectionPanda from '$lib/components/landing/SectionPanda.svelte'
  import SectionPrinciples from '$lib/components/landing/SectionPrinciples.svelte'
  import SectionProjects from '$lib/components/landing/SectionProjects.svelte'
  import { getToastStore } from '$lib/ui/stores'
  import { onMount } from 'svelte'

  const toastStore = getToastStore()

  onMount(async () => {
    await new Promise((res) => setTimeout(res, 3000))

    // Announcements are decoration on a static page: an unreachable canister
    // should stay silent, not surface as an unhandled rejection.
    let notifications: Awaited<ReturnType<typeof luckyPoolAPI.notifications>>
    try {
      notifications = await luckyPoolAPI.notifications()
    } catch (err) {
      console.warn('Failed to load notifications:', err)
      return
    }

    for (const n of notifications) {
      toastStore.trigger({
        autohide: n.timeout != 0,
        timeout: n.timeout,
        hideDismiss: !n.dismiss,
        classes: 'bg-black',
        message: n.message
      })
    }
  })
</script>

<SectionHero />
<SectionPanda />
<SectionProjects />
<SectionDao />
<SectionPrinciples />
<SectionArchive />
<SectionCta />

<footer
  id="page-footer"
  class="flex-none px-3 pt-4 pb-6 md:px-6 md:pt-8 md:pb-10"
>
  <div class="sheet mx-auto w-full max-w-6xl [&>div]:border-t-0">
    <Registration />
    <PageFooter />
  </div>
</footer>
