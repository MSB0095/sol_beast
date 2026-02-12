# SOL BEAST Documentation

This directory contains the full documentation website for SOL BEAST, built with VitePress.

## 🎨 Features

- **7 Cyberpunk Themes**: Matching the frontend dashboard aesthetic
  - 🟢 MATRIX (green)
  - 💎 NEON (emerald)
  - 🔵 CYBER (cyan)
  - 🟣 PLASMA (purple)
  - 💗 LASER (rose)
  - 🟡 GOLD (amber)
  - 🔷 TRON (cyan)

- **Comprehensive Guides**: Getting started, configuration, trading strategies
- **API Reference**: Complete REST and WebSocket documentation
- **Search**: Built-in local search functionality
- **Responsive**: Works on desktop and mobile

## 📦 Installation

```bash
cd docs
npm install
```

## 🚀 Development

Run the documentation site locally:

```bash
npm run docs:dev
```

Visit `http://localhost:5173` (default VitePress port).

## 🏗️ Building

Build the static site:

```bash
npm run docs:build
```

Output will be in `.vitepress/dist`.

## 📝 Preview Production Build

```bash
npm run docs:preview
```

## 📁 Structure

```
docs/
├── .vitepress/
│   ├── config.ts              # VitePress configuration
│   └── theme/
│       ├── index.ts           # Custom theme setup
│       ├── style.css          # Theme styles (7 color schemes)
│       └── components/
│           └── ThemeSwitcher.vue  # Theme switcher component
├── guide/
│   ├── introduction.md        # What is SOL BEAST
│   ├── getting-started.md     # Quick start guide
│   ├── installation.md        # Detailed installation
│   ├── configuration.md       # Configuration reference
│   ├── helius-sender.md       # Helius integration
│   ├── trading-parameters.md  # Trading strategy params
│   ├── dashboard.md           # Frontend guide
│   ├── architecture.md        # Technical overview
│   ├── themes.md              # Color themes
│   ├── troubleshooting.md     # Common issues
│   ├── faq.md                 # Frequently asked questions
│   └── contributing.md        # Contribution guide
├── api/
│   ├── endpoints.md           # REST API reference
│   └── websocket.md           # WebSocket events
├── advanced/
│   ├── strategies.md          # Trading strategies
│   ├── risk-management.md     # Risk management
│   └── performance.md         # Performance tuning
├── index.md                   # Homepage
├── package.json
└── README.md                  # This file
```

## 🎨 Theming

The documentation site uses the exact same theming system as the frontend dashboard:

- **CSS Variables**: All themes defined in `.vitepress/theme/style.css`
- **Theme Switcher**: Vue component in `.vitepress/theme/components/ThemeSwitcher.vue`
- **Persistence**: Saves theme preference to localStorage
- **Matching**: Color schemes match frontend exactly

## 📖 Adding Content

1. Create a new `.md` file in the appropriate directory
2. Add frontmatter if needed
3. Update `.vitepress/config.ts` sidebar navigation
4. Build and preview

Example:
```markdown
# Page Title

Your content here...

::: tip
Helpful tip for users
:::
```

## 🔗 Links

- **Live Docs**: (Coming soon - will be hosted on GitHub Pages)
- **Main README**: [../README.md](../README.md)
- **GitHub**: [https://github.com/MSB0095/sol_beast](https://github.com/MSB0095/sol_beast)

## 🤝 Contributing

Contributions to documentation are welcome! Please:

1. Follow the existing structure and style
2. Use clear, concise language
3. Add code examples where helpful
4. Test builds before submitting PR

## 📄 License

Same as main project - MIT License
