<script lang="ts">
  import IconArrowRightUp from '$lib/components/icons/IconArrowRightUp.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import Reveal from '$lib/components/ui/Reveal.svelte'
  import { LINKS } from '$lib/site'

  const actions = [
    { label: 'Explore GitHub', href: LINKS.github, icon: IconGithub },
    { label: 'Join Governance', href: LINKS.snsProposals, icon: IconPanda },
    { label: 'Follow @ICPandaDAO', href: LINKS.x, icon: IconX }
  ]
</script>

<section
  id="community"
  class="border-ink/15 bg-ink text-paper scroll-mt-16 border-t"
>
  <div class="mx-auto w-full max-w-6xl px-5 py-16 md:px-10 md:py-24">
    <div class="border-paper/15 flex items-center gap-4 border-b pb-4">
      <span
        class="font-mono text-xs leading-none font-medium tracking-[0.18em] uppercase tabular-nums"
        >07</span
      >
      <span class="bg-paper/20 h-px flex-1"></span>
      <span
        class="text-paper/50 font-mono text-xs leading-none font-medium tracking-[0.18em] uppercase"
        >ICPanda DAO</span
      >
    </div>

    <Reveal>
      <h2
        class="font-display mt-14 text-[clamp(3rem,10vw,7rem)] leading-[0.9] font-black tracking-[-0.04em]"
      >
        Build the commons.
      </h2>
    </Reveal>

    <div class="mt-12 grid grid-cols-1 gap-10 lg:grid-cols-12">
      <Reveal delay={80} class="lg:col-span-7">
        <p class="text-paper/80 text-lg leading-relaxed text-pretty">
          Protocols become infrastructure when others can build on them.
        </p>
        <p class="text-paper/60 mt-4 leading-relaxed text-pretty">
          Explore the code. Participate in governance. Test the protocols.
          Challenge our assumptions. Build something we did not imagine.
        </p>
      </Reveal>

      <Reveal delay={140} class="lg:col-span-5">
        <ul class="border-paper/15 border-t">
          {#each actions as action (action.label)}
            <li>
              <a
                class="group border-paper/15 hover:text-panda flex items-center justify-between gap-4 border-b py-5 transition-colors duration-200"
                href={action.href}
                target="_blank"
                rel="noreferrer"
              >
                <span class="flex items-center gap-4">
                  <span class="*:size-5" aria-hidden="true">
                    <action.icon />
                  </span>
                  <span class="font-mono text-sm font-medium">
                    {action.label}
                  </span>
                </span>
                <span
                  class="text-paper/40 group-hover:text-panda transition-transform duration-200 *:size-5 group-hover:translate-x-0.5 group-hover:-translate-y-0.5"
                  aria-hidden="true"><IconArrowRightUp /></span
                >
              </a>
            </li>
          {/each}
        </ul>
      </Reveal>
    </div>
  </div>
</section>
