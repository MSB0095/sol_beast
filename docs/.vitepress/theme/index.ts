// https://vitepress.dev/guide/custom-theme
import { h } from 'vue'
import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import './style.css'
import ThemeSwitcher from './components/ThemeSwitcher.vue'
import BeastLogo from './components/BeastLogo.vue'
import BeastBanner from './components/BeastBanner.vue'

export default {
  extends: DefaultTheme,
  Layout: () => {
    return h(DefaultTheme.Layout, null, {
      'nav-bar-content-after': () => h(ThemeSwitcher)
    })
  },
  enhanceApp({ app, router, siteData }) {
    // Register brand components globally for use in markdown
    app.component('BeastLogo', BeastLogo)
    app.component('BeastBanner', BeastBanner)

    // Add theme class to document on mount
    if (typeof window !== 'undefined') {
      const savedTheme = localStorage.getItem('colorTheme') || 'sol-green'
      document.documentElement.setAttribute('data-color-theme', savedTheme)
    }
  }
} satisfies Theme
