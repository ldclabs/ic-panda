import autoprefixer from 'autoprefixer'
import cssnano from 'cssnano'
import tailwindcss from 'tailwindcss'

const config = {
  plugins: [
    //Some plugins, like tailwindcss/nesting, need to run before Tailwind,
    tailwindcss(),
    //But others, like autoprefixer, need to run after,
    autoprefixer,
    cssnano({ preset: 'default' })
  ]
}

export default config
