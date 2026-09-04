import forms from '@tailwindcss/forms'
import colors from 'tailwindcss/colors'

/**
 * The palette lives in `src/app.css` as `--color-<name>-<shade>` custom
 * properties holding space-separated RGB channels — the same shape the
 * Skeleton theme used, so the compatibility layer in that file can keep
 * writing `rgb(var(--color-primary-500) / 0.2)` verbatim.
 *
 * Here we only point Tailwind at those variables. `<alpha-value>` lets the
 * opacity modifiers (`bg-surface-500/20`) keep working.
 */
const shades = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900] as const

function scale(name: string): Record<string, string> {
  return Object.fromEntries(
    shades.map((shade) => [
      String(shade),
      `rgb(var(--color-${name}-${shade}) / <alpha-value>)`
    ])
  )
}

const config = {
  darkMode: 'selector',
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    colors: {
      transparent: 'transparent',
      current: 'currentColor',
      panda: '#11c291',
      gold: '#fbbf24',
      white: colors.white,
      black: colors.black,
      pink: colors.pink,
      orange: colors.orange,
      amber: colors.amber,
      red: colors.red,
      neutral: colors.neutral,
      primary: scale('primary'),
      secondary: scale('secondary'),
      tertiary: scale('tertiary'),
      success: scale('success'),
      warning: scale('warning'),
      error: scale('error'),
      surface: scale('surface')
    },
    extend: {}
  },
  plugins: [forms],
  safelist: ['text-black']
}

export default config
