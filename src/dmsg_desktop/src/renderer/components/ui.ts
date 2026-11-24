import { clsx, type ClassValue } from 'clsx'
import { extendTailwindMerge } from 'tailwind-merge'

/** The type scale in `app.css`. tailwind-merge learns these `text-*` sizes and the `shadow-*` names, so a later color keeps them. */
export const typeScale = ['title-1', 'title-2', 'title-3', 'body', 'callout', 'caption', 'code'] as const

const twMerge = extendTailwindMerge({ extend: { theme: { text: [...typeScale], shadow: ['control', 'pop'] } } })

/**
 * Class names for the component kit, as anda-bot's `ui.ts` composes them:
 * views pick a variant and size and never restyle a control by hand.
 */
export const cn = (...inputs: ClassValue[]) => twMerge(clsx(inputs))

export const modalOpen = () => !!document.querySelector('[aria-modal="true"]:not([data-state="closed"])')

export type ButtonVariant = 'primary' | 'secondary' | 'subtle' | 'ghost' | 'danger'
export type ButtonSize = 'xs' | 'sm' | 'md' | 'lg'

const buttonBase =
  'relative inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap rounded-lg border font-medium select-none outline-none transition-[background-color,border-color,color,box-shadow] duration-150 focus-visible:ring-3 focus-visible:ring-focus/35 focus-visible:outline-none active:not-disabled:translate-y-px disabled:cursor-not-allowed disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0'

const buttonVariants: Record<ButtonVariant, string> = {
  primary: 'border-forest bg-forest text-on-forest hover:not-disabled:border-forest-hover hover:not-disabled:bg-forest-hover',
  // Raised a little off the page by a hairline and a soft shadow, as a Mac push button, instead of a drawn frame.
  secondary: 'border-transparent bg-surface text-ink shadow-control hover:not-disabled:bg-surface-2 aria-expanded:bg-surface-2',
  // An action inside a row or a line of text: a tint, no edge.
  subtle: 'border-transparent bg-wash text-ink hover:not-disabled:bg-pale aria-expanded:bg-pale',
  ghost: 'border-transparent bg-transparent text-muted hover:not-disabled:bg-wash hover:not-disabled:text-ink aria-expanded:bg-wash',
  danger: 'border-transparent bg-surface text-danger shadow-control hover:not-disabled:bg-danger-pale'
}

const buttonSizes: Record<ButtonSize, string> = {
  xs: 'h-7 px-2 text-callout [&_svg]:size-4',
  sm: 'h-8 px-2.5 text-callout [&_svg]:size-4',
  md: 'h-8 px-3 text-body [&_svg]:size-4.5',
  lg: 'h-10 px-4 text-body [&_svg]:size-5'
}

export function buttonClass(variant: ButtonVariant = 'secondary', size: ButtonSize = 'md', className?: ClassValue) {
  return cn(buttonBase, buttonVariants[variant], buttonSizes[size], className)
}

/** Square tools keep their hit area and glyph size together; large closes a panel. */
export function iconButtonClass(variant: ButtonVariant = 'ghost', size: 'md' | 'lg' = 'md', className?: ClassValue) {
  return buttonClass(variant, 'md', cn(size === 'lg' ? 'size-9 [&_svg]:size-5' : 'size-8 [&_svg]:size-4.5', 'p-0', className))
}

/** A floating panel of choices: the dropdowns, the suggestions and the language menu. */
export const menuContentClass =
  'z-50 max-h-[min(var(--bits-floating-available-height),360px)] min-w-(--bits-floating-anchor-width) max-w-[min(420px,calc(100vw-16px))] overflow-y-auto rounded-xl border border-line bg-surface p-1 text-ink shadow-pop outline-none data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95'

export const menuItemClass =
  'flex cursor-default items-center justify-between gap-4 rounded-lg px-2.5 py-1.5 text-body outline-none select-none [overflow-wrap:anywhere] data-highlighted:bg-wash data-[state=checked]:font-medium data-[state=checked]:text-forest data-disabled:cursor-progress data-disabled:opacity-60'
