import { defineConfig } from 'vitepress'

export default defineConfig({
  title: "SOL BEAST",
  description: "Ultra-Fast Solana Token Sniping Bot - Documentation",
  base: '/',
  
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' }],
    ['link', { rel: 'icon', type: 'image/png', sizes: '256x256', href: '/favicon.png' }],
    ['link', { rel: 'icon', type: 'image/x-icon', href: '/favicon.ico' }],
    ['link', { rel: 'apple-touch-icon', sizes: '180x180', href: '/apple-touch-icon.png' }],
    ['meta', { name: 'theme-color', content: '#00ff41' }],
    ['meta', { property: 'og:title', content: 'SOL BEAST Documentation' }],
    ['meta', { property: 'og:description', content: 'Ultra-Fast Solana Token Sniping Bot' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.googleapis.com' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' }],
    ['link', { href: 'https://fonts.googleapis.com/css2?family=Orbitron:wght@400;500;600;700;800;900&family=JetBrains+Mono:wght@300;400;500;600;700&family=Rajdhani:wght@300;400;500;600;700&family=Share+Tech+Mono&display=swap', rel: 'stylesheet' }],
  ],

  themeConfig: {
    // Logo is rendered via BeastLogo.vue component in theme/index.ts
    // logo: '/logo.svg',
    
    nav: [
      { text: 'Home', link: '/' },
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'API', link: '/api/endpoints' },
      { text: 'GitHub', link: 'https://github.com/MSB0095/sol_beast' }
    ],

    sidebar: [
      {
        text: 'Introduction',
        items: [
          { text: 'What is SOL BEAST?', link: '/guide/introduction' },
          { text: 'Getting Started', link: '/guide/getting-started' },
          { text: 'Installation', link: '/guide/installation' },
        ]
      },
      {
        text: 'Configuration',
        items: [
          { text: 'Basic Configuration', link: '/guide/configuration' },
          { text: 'Trading Parameters', link: '/guide/trading-parameters' },
          { text: 'Helius Sender', link: '/guide/helius-sender' },
        ]
      },
      {
        text: 'Features',
        items: [
          { text: 'Architecture', link: '/guide/architecture' },
          { text: 'Frontend Dashboard', link: '/guide/dashboard' },
          { text: 'Themes', link: '/guide/themes' },
        ]
      },
      {
        text: 'Advanced',
        items: [
          { text: 'Trading Strategies', link: '/advanced/strategies' },
          { text: 'Risk Management', link: '/advanced/risk-management' },
          { text: 'Performance Tuning', link: '/advanced/performance' },
        ]
      },
      {
        text: 'API Reference',
        items: [
          { text: 'REST Endpoints', link: '/api/endpoints' },
          { text: 'WebSocket Events', link: '/api/websocket' },
        ]
      },
      {
        text: 'Help',
        items: [
          { text: 'Troubleshooting', link: '/guide/troubleshooting' },
          { text: 'FAQ', link: '/guide/faq' },
          { text: 'Contributing', link: '/guide/contributing' },
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/MSB0095/sol_beast' },
      { icon: 'discord', link: 'https://discord.gg/solbeast' },
      { icon: 'twitter', link: 'https://x.com/Sol__Beast' }
    ],

    footer: {
      message: 'Released under the MIT License.',
      copyright: 'Copyright © 2024 SOL BEAST Team'
    },

    search: {
      provider: 'local'
    },

    editLink: {
      pattern: 'https://github.com/MSB0095/sol_beast/edit/main/docs/:path',
      text: 'Edit this page on GitHub'
    },

    lastUpdated: {
      text: 'Updated at',
      formatOptions: {
        dateStyle: 'full',
        timeStyle: 'medium'
      }
    }
  }
})
