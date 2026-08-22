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
  class="scroll-mt-16 border-t border-ink/15 bg-ink text-paper"
>
  <div class="mx-auto w-full max-w-[1180px] px-5 py-16 md:px-10 md:py-24">
    <div class="flex items-center gap-4 border-b border-paper/15 pb-4">
      <span
        class="font-mono text-xs font-medium uppercase tabular-nums leading-none tracking-[0.18em]"
        >07</span
      >
      <span class="h-px flex-1 bg-paper/20"></span>
      <span
        class="font-mono text-xs font-medium uppercase leading-none tracking-[0.18em] text-paper/50"
        >ICPanda DAO</span
      >
    </div>

    <Reveal>
      <h2
        class="mt-14 font-display text-[clamp(3rem,10vw,7rem)] font-black leading-[0.9] tracking-[-0.04em]"
      >
        Build the commons.
      </h2>
    </Reveal>

    <div class="mt-12 grid grid-cols-1 gap-10 lg:grid-cols-12">
      <Reveal delay={80} class="lg:col-span-7">
        <p class="text-pretty text-lg leading-relaxed text-paper/80">
          Protocols become infrastructure when others can build on them.
        </p>
        <p class="mt-4 text-pretty leading-relaxed text-paper/60">
          Explore the code. Participate in governance. Test the protocols.
          Challenge our assumptions. Build something we did not imagine.
        </p>
      </Reveal>

      <Reveal delay={140} class="lg:col-span-5">
        <ul class="border-t border-paper/15">
          {#each actions as action (action.label)}
            <li>
              <a
                class="group flex items-center justify-between gap-4 border-b border-paper/15 py-5 transition-colors duration-200 hover:text-panda"
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
                  class="text-paper/40 transition-transform duration-200 *:size-5 group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-panda"
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
