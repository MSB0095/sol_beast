// https://vitepress.dev/guide/custom-theme
import { h } from 'vue'
import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import './style.css'
import ThemeSwitcher from './components/ThemeSwitcher.vue'

export default {
  extends: DefaultTheme,
  Layout: () => {
    return h(DefaultTheme.Layout, null, {
      'nav-bar-content-after': () => h(ThemeSwitcher)
    })
  },
  enhanceApp({ app, router, siteData }) {
    // Add theme class to document on mount
    if (typeof window !== 'undefined') {
      const savedTheme = localStorage.getItem('colorTheme') || 'sol-green'
      document.documentElement.setAttribute('data-color-theme', savedTheme)
    }
  }
} satisfies Theme
