<script lang="ts">
  import IconArrowDownLine from '$lib/components/icons/IconArrowDownLine.svelte'
  import HeroSchematic from './HeroSchematic.svelte'
</script>

<section id="hero" class="relative">
  <div
    class="mx-auto w-full max-w-6xl px-5 pt-14 pb-16 md:px-10 md:pt-20 md:pb-24"
  >
    <p class="eyebrow">
      Open Source <span class="text-panda">·</span> On-chain Governed
      <span class="text-panda">·</span> Built on ICP
    </p>

    <h1 class="display mt-7 text-[clamp(2.25rem,7.2vw,5.6rem)]">
      From Sovereign Minds<br />to Sovereign Markets.<span
        class="caret bg-panda ml-3 inline-block h-[0.58em] w-[0.26em] align-baseline"
        aria-hidden="true"
      ></span>
    </h1>

    <div
      class="border-ink/15 mt-12 grid grid-cols-1 gap-10 border-t pt-10 lg:grid-cols-12 lg:gap-12"
    >
      <div class="lg:col-span-7">
        <p
          class="text-ink-70 max-w-2xl text-lg leading-relaxed text-pretty md:text-xl"
        >
          ICPanda DAO is an on-chain builder collective creating open
          infrastructure for
          <span class="text-ink font-medium">persistent AI cognition</span>
          and
          <span class="text-ink font-medium"
            >verifiable crypto capital formation</span
          >.
        </p>

        <div class="mt-10 flex flex-col gap-3 sm:flex-row">
          <a class="btn-ink" href="#panda">Explore PANDA</a>
          <a class="btn-outline" href="#projects">
            What We Build
            <span class="*:size-4"><IconArrowDownLine /></span>
          </a>
        </div>

        <p class="text-ink-70 mt-10 max-w-md font-mono text-xs leading-relaxed">
          Governed by the community through PANDA and the Internet Computer SNS.
        </p>
      </div>

      <div class="lg:col-span-5">
        <HeroSchematic />
      </div>
    </div>
  </div>
</section>
