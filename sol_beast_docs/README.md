# Sol Beast Documentation

This directory contains the comprehensive documentation for Sol Beast, built using [mdBook](https://rust-lang.github.io/mdBook/).

## 📖 About

The documentation covers:
- Getting Started guides
- Architecture and design
- Complete configuration reference
- Trading features and strategies
- API documentation
- Troubleshooting and FAQ

## 🔨 Building Locally

### Prerequisites

Install mdBook:
```bash
cargo install mdbook --version 0.4.40
```

### Build

```bash
# From project root
./build-docs.sh

# Or manually
cd sol_beast_docs
mdbook build
```

The built documentation will be in `sol_beast_docs/book/`.

### Serve Locally

```bash
cd sol_beast_docs
mdbook serve --open
```

This will start a local server at `http://localhost:3000` and open your browser.

## 📝 Editing Documentation

### Structure

- `src/` - Markdown source files
- `src/SUMMARY.md` - Table of contents (controls sidebar)
- `theme/` - Custom CSS and JavaScript
- `book.toml` - Configuration file
- `book/` - Built output (ignored by git)

### Adding Pages

1. Create markdown file in appropriate `src/` subdirectory
2. Add entry to `src/SUMMARY.md`
3. Rebuild with `mdbook build`

### Styling

The documentation uses a custom cyber/electric theme that matches the Sol Beast frontend:

- Custom colors defined in `theme/custom.css`
- Additional JavaScript effects in `theme/custom.js`
- Electric green accent (#00ff41) theme throughout
- Glowing borders and cyber effects

## 🚀 Deployment

The documentation is automatically built and deployed with the main application:

1. GitHub Actions workflow (`.github/workflows/deploy.yml`)
2. Installs mdBook
3. Builds documentation with `./build-docs.sh`
4. Copies to `frontend/dist/sol_beast_docs/`
5. Deployed to GitHub Pages with the frontend

## 📚 Documentation Sections

- **Getting Started** - Installation and quickstart
- **Architecture** - System design and components
- **Configuration** - Complete settings reference
- **Trading Features** - Bot operation and strategies
- **Helius Integration** - Advanced transaction submission
- **Frontend Guide** - UI usage and features
- **API Reference** - REST and WASM APIs
- **Development** - Contributing and building
- **Deployment** - Production deployment
- **Troubleshooting** - Common issues and solutions
- **Appendix** - Glossary, resources, changelog

## 🎨 Theme Customization

To modify the theme:

1. Edit `theme/custom.css` for styling
2. Edit `theme/custom.js` for interactive features
3. Rebuild to see changes

Color variables:
```css
--sol-beast-accent: #00ff41;  /* Main accent color */
--glow-color: rgba(0, 255, 65, 0.6);  /* Glow effects */
--sol-beast-bg-primary: #000000;  /* Background */
```

## 🤝 Contributing

Contributions to documentation are welcome!

1. Follow the existing structure and style
2. Use clear, concise language
3. Include code examples where helpful
4. Test locally before submitting
5. Update SUMMARY.md if adding pages

## 📄 License

Same as Sol Beast project - see main LICENSE file.
