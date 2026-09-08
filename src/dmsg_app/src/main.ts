import { mount } from 'svelte'
import App from './App.svelte'
import Popup from './lib/components/Popup.svelte'
import './app.css'

const target = document.getElementById('app')!
mount(document.body.dataset.surface === 'popup' ? Popup : App, { target })
