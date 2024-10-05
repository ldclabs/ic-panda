import { skeleton } from '@skeletonlabs/tw-plugin'
import forms from '@tailwindcss/forms'
import { join } from 'path'
import colors from 'tailwindcss/colors'
import { ICPandaTheme } from './icpanda-theme'

const config = {
  darkMode: 'selector',
  content: [
    './src/lib/**/*.{html,svelte,ts}',
    './src/routes/**/*.{html,svelte,ts}',
    join(
      require.resolve('@skeletonlabs/skeleton'),
      '../**/*.{html,svelte,js,ts}'
    )
  ],
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
      neutral: colors.neutral
    },
    extend: {}
  },
  plugins: [
    forms,
    skeleton({
      themes: {
        custom: [ICPandaTheme]
      }
    })
  ],
  safelist: ['text-black']
}

export default config
