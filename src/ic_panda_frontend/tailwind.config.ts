import forms from '@tailwindcss/forms'
import colors from 'tailwindcss/colors'

const sansStack = [
  'Archivo',
  '-apple-system',
  'BlinkMacSystemFont',
  'system-ui',
  'Segoe UI',
  'Noto Sans',
  'Roboto',
  'Helvetica',
  'Arial',
  'sans-serif',
  'Apple Color Emoji',
  'Segoe UI Emoji'
]

const monoStack = [
  'IBM Plex Mono',
  'ui-monospace',
  'SFMono-Regular',
  'SF Mono',
  'Menlo',
  'Consolas',
  'monospace'
]

/** @type {import('tailwindcss').Config} */
const config = {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    colors: {
      transparent: 'transparent',
      current: 'currentColor',
      panda: '#11c291',
      gold: '#fbbf24',
      gray: '#0a0a0a',
      // v2 brand ground: bone paper + ink, with a few tuned ink tints so
      // secondary copy stays legible without introducing a grey scale.
      paper: '#f2f1ec',
      ink: {
        DEFAULT: '#0b0b0b',
        90: 'rgb(11 11 11 / 0.9)',
        70: 'rgb(11 11 11 / 0.7)',
        50: 'rgb(11 11 11 / 0.5)',
        30: 'rgb(11 11 11 / 0.3)'
      },
      white: colors.white,
      black: colors.black,
      pink: colors.pink,
      orange: colors.orange,
      amber: colors.amber,
      indigo: colors.indigo,
      red: colors.red
    },
    extend: {
      fontFamily: {
        sans: sansStack,
        display: sansStack,
        mono: monoStack
      }
    }
  },
  plugins: [forms],
  safelist: ['text-black']
}

export default config
