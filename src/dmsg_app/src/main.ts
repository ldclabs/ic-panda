import { mount } from 'svelte'
import './app.css'

const target = document.getElementById('app')!
const { default: component } =
  document.body.dataset.surface === 'popup'
    ? await import('./lib/components/Popup.svelte')
    : await import('./App.svelte')
mount(component, { target })
