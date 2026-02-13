#!/usr/bin/env node
/**
 * generate-brand-assets.mjs
 *
 * Generates static SVG and PNG brand assets for Sol Beast in all 7 theme colours.
 *
 * Output → assets/brand/
 *   ├── logo/svg/logo-{theme}.svg          (7 files)
 *   ├── logo/png/logo-{theme}-{size}.png   (28 files: 7 themes × 4 sizes)
 *   ├── banner/svg/banner-{theme}.svg      (7 files)
 *   ├── banner/png/banner-{theme}-1024.png (7 files)
 *   └── favicon.ico                        (from matrix-green logo at 32px)
 *
 * Usage:
 *   node scripts/generate-brand-assets.mjs
 *
 * Dependencies (auto-installed if missing):
 *   @resvg/resvg-js  — SVG → PNG rasteriser (no browser needed)
 *   png-to-ico       — PNG → ICO converter
 */

import { mkdir, writeFile, readFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { execSync } from 'node:child_process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const ROOT = join(__dirname, '..')
const OUT = join(ROOT, 'assets', 'brand')

// ─── Ensure dependencies ────────────────────────────────────────────
// (handled inside main())

// ─── Theme palette ──────────────────────────────────────────────────
const THEMES = [
  { id: 'matrix',  accent: '#00ff41', accentHover: '#00cc33', glow: 'rgba(0,255,65,0.6)' },
  { id: 'neon',    accent: '#10ffb0', accentHover: '#00e69c', glow: 'rgba(16,255,176,0.6)' },
  { id: 'cyber',   accent: '#00d9ff', accentHover: '#00b8d9', glow: 'rgba(0,217,255,0.6)' },
  { id: 'plasma',  accent: '#d946ef', accentHover: '#c026d3', glow: 'rgba(217,70,239,0.6)' },
  { id: 'laser',   accent: '#ff0062', accentHover: '#e6004f', glow: 'rgba(255,0,98,0.6)' },
  { id: 'gold',    accent: '#ffb000', accentHover: '#e6a000', glow: 'rgba(255,176,0,0.6)' },
  { id: 'tron',    accent: '#00ffff', accentHover: '#00e5e5', glow: 'rgba(0,255,255,0.6)' },
]

const LOGO_SIZES = [128, 256, 512, 1024]
const BANNER_WIDTH = 1920   // high-res banner
const BANNER_HEIGHT = 480

// ─── Logo SVG template ──────────────────────────────────────────────
function logoSvg(accent, accentO15, accentO03) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120" fill="none">
  <defs>
    <radialGradient id="eg" cx="50%" cy="50%" r="50%">
      <stop offset="0%" stop-color="${accent}" stop-opacity="1"/>
      <stop offset="60%" stop-color="${accent}" stop-opacity="0.6"/>
      <stop offset="100%" stop-color="${accent}" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="hg" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="${accent}" stop-opacity="1"/>
      <stop offset="100%" stop-color="${accent}" stop-opacity="0.3"/>
    </linearGradient>
    <linearGradient id="hf" x1="50%" y1="0%" x2="50%" y2="100%">
      <stop offset="0%" stop-color="${accent}" stop-opacity="${accentO15}"/>
      <stop offset="100%" stop-color="${accent}" stop-opacity="${accentO03}"/>
    </linearGradient>
  </defs>
  <!-- Head -->
  <path d="M60 14 C42 14,28 24,24 40 L22 52 C18 58,16 64,16 70 C16 78,20 84,26 88 L26 94 C26 97,28 100,32 100 L42 100 L42 106 C42 108,44 110,46 110 L74 110 C76 110,78 108,78 106 L78 100 L88 100 C92 100,94 97,94 94 L94 88 C100 84,104 78,104 70 C104 64,102 58,98 52 L96 40 C92 24,78 14,60 14 Z" stroke="${accent}" stroke-width="2.5" fill="url(#hf)"/>
  <path d="M60 20 C46 20,34 28,30 42 L28 52 C24 57,22 62,22 68 C22 74,25 79,30 83" stroke="${accent}" stroke-width="1" stroke-opacity="0.25" fill="none"/>
  <path d="M60 20 C74 20,86 28,90 42 L92 52 C96 57,98 62,98 68 C98 74,95 79,90 83" stroke="${accent}" stroke-width="1" stroke-opacity="0.25" fill="none"/>
  <!-- Horns -->
  <path d="M34 34 C30 26,22 14,14 6 C18 12,22 22,28 30 Z" fill="url(#hg)"/>
  <line x1="28" y1="28" x2="20" y2="14" stroke="${accent}" stroke-width="0.8" stroke-opacity="0.4"/>
  <line x1="30" y1="30" x2="24" y2="20" stroke="${accent}" stroke-width="0.6" stroke-opacity="0.3"/>
  <path d="M86 34 C90 26,98 14,106 6 C102 12,98 22,92 30 Z" fill="url(#hg)"/>
  <line x1="92" y1="28" x2="100" y2="14" stroke="${accent}" stroke-width="0.8" stroke-opacity="0.4"/>
  <line x1="90" y1="30" x2="96" y2="20" stroke="${accent}" stroke-width="0.6" stroke-opacity="0.3"/>
  <!-- Ears -->
  <path d="M28 42 C22 38,18 32,20 26 C22 32,26 36,30 40" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.6"/>
  <path d="M92 42 C98 38,102 32,100 26 C98 32,94 36,90 40" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.6"/>
  <!-- Brows -->
  <path d="M30 44 C34 40,40 38,48 40" stroke="${accent}" stroke-width="2.5" stroke-linecap="round" fill="none"/>
  <path d="M90 44 C86 40,80 38,72 40" stroke="${accent}" stroke-width="2.5" stroke-linecap="round" fill="none"/>
  <!-- Left eye -->
  <ellipse cx="42" cy="52" rx="10" ry="8" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.3"/>
  <ellipse cx="42" cy="52" rx="8" ry="6" fill="url(#eg)" opacity="0.3"/>
  <ellipse cx="42" cy="52" rx="6" ry="5" fill="${accent}"/>
  <ellipse cx="42" cy="52" rx="1.8" ry="4.5" fill="black"/>
  <circle cx="39" cy="50" r="1.5" fill="white" opacity="0.7"/>
  <!-- Right eye -->
  <ellipse cx="78" cy="52" rx="10" ry="8" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.3"/>
  <ellipse cx="78" cy="52" rx="8" ry="6" fill="url(#eg)" opacity="0.3"/>
  <ellipse cx="78" cy="52" rx="6" ry="5" fill="${accent}"/>
  <ellipse cx="78" cy="52" rx="1.8" ry="4.5" fill="black"/>
  <circle cx="75" cy="50" r="1.5" fill="white" opacity="0.7"/>
  <!-- Muzzle -->
  <path d="M52 58 L60 66 L68 58" stroke="${accent}" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="55" cy="64" r="2" stroke="${accent}" stroke-width="1.2" fill="none"/>
  <circle cx="65" cy="64" r="2" stroke="${accent}" stroke-width="1.2" fill="none"/>
  <!-- Cheek scales -->
  <path d="M28 60 C30 58,34 56,32 62" stroke="${accent}" stroke-width="0.8" stroke-opacity="0.3" fill="none"/>
  <path d="M30 66 C32 64,36 62,34 68" stroke="${accent}" stroke-width="0.8" stroke-opacity="0.3" fill="none"/>
  <path d="M92 60 C90 58,86 56,88 62" stroke="${accent}" stroke-width="0.8" stroke-opacity="0.3" fill="none"/>
  <path d="M90 66 C88 64,84 62,86 68" stroke="${accent}" stroke-width="0.8" stroke-opacity="0.3" fill="none"/>
  <!-- Jaw + teeth -->
  <path d="M32 74 C38 72,48 71,60 72 C72 71,82 72,88 74" stroke="${accent}" stroke-width="2" fill="none"/>
  <path d="M38 74 L36 84 L40 74" fill="${accent}" fill-opacity="0.9" stroke="${accent}" stroke-width="1"/>
  <path d="M44 74 L43 80 L46 74" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
  <path d="M50 73 L49 78 L52 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
  <path d="M56 73 L55 79 L58 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
  <path d="M62 73 L61 79 L64 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
  <path d="M68 73 L67 78 L70 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
  <path d="M74 74 L73 80 L76 74" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
  <path d="M80 74 L82 84 L78 74" fill="${accent}" fill-opacity="0.9" stroke="${accent}" stroke-width="1"/>
  <!-- Lower jaw -->
  <path d="M34 76 C40 82,50 84,60 84 C70 84,80 82,86 76" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.5"/>
  <path d="M42 84 L44 78 L46 84" fill="${accent}" fill-opacity="0.5" stroke="${accent}" stroke-width="0.6"/>
  <path d="M52 84 L54 79 L56 84" fill="${accent}" fill-opacity="0.5" stroke="${accent}" stroke-width="0.6"/>
  <path d="M62 84 L64 79 L66 84" fill="${accent}" fill-opacity="0.5" stroke="${accent}" stroke-width="0.6"/>
  <path d="M72 84 L74 78 L76 84" fill="${accent}" fill-opacity="0.5" stroke="${accent}" stroke-width="0.6"/>
  <!-- Forehead scales -->
  <path d="M50 26 L54 30 L58 26 L62 30 L66 26 L70 30" stroke="${accent}" stroke-width="0.7" stroke-opacity="0.2" fill="none"/>
  <path d="M46 32 L50 36 L54 32 L58 36 L62 32 L66 36 L70 32 L74 36" stroke="${accent}" stroke-width="0.7" stroke-opacity="0.15" fill="none"/>
  <!-- Side jaw ridges -->
  <path d="M24 70 L28 74 L26 78" stroke="${accent}" stroke-width="1.2" stroke-opacity="0.4" fill="none"/>
  <path d="M96 70 L92 74 L94 78" stroke="${accent}" stroke-width="1.2" stroke-opacity="0.4" fill="none"/>
</svg>`
}

// ─── Banner SVG template ────────────────────────────────────────────
function bannerSvg(accent, accentO15, accentO03) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 120" fill="none">
  <defs>
    <linearGradient id="bhg" x1="0%" y1="0%" x2="0%" y2="100%">
      <stop offset="0%" stop-color="${accent}" stop-opacity="1"/>
      <stop offset="100%" stop-color="${accent}" stop-opacity="0.3"/>
    </linearGradient>
    <linearGradient id="bhf" x1="50%" y1="0%" x2="50%" y2="100%">
      <stop offset="0%" stop-color="${accent}" stop-opacity="${accentO15}"/>
      <stop offset="100%" stop-color="${accent}" stop-opacity="${accentO03}"/>
    </linearGradient>
    <linearGradient id="btg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="${accent}" stop-opacity="1"/>
      <stop offset="100%" stop-color="${accent}" stop-opacity="0.7"/>
    </linearGradient>
  </defs>
  <!-- Background -->
  <rect width="480" height="120" fill="black" rx="8"/>
  <rect x="1" y="1" width="478" height="118" rx="7" stroke="${accent}" stroke-width="2" fill="none" stroke-opacity="0.6"/>
  <rect x="4" y="4" width="472" height="112" rx="5" stroke="${accent}" stroke-width="0.5" fill="none" stroke-opacity="0.15"/>
  <!-- Beast logo -->
  <g transform="translate(10,5) scale(0.92)">
    <path d="M60 14 C42 14,28 24,24 40 L22 52 C18 58,16 64,16 70 C16 78,20 84,26 88 L26 94 C26 97,28 100,32 100 L42 100 L42 106 C42 108,44 110,46 110 L74 110 C76 110,78 108,78 106 L78 100 L88 100 C92 100,94 97,94 94 L94 88 C100 84,104 78,104 70 C104 64,102 58,98 52 L96 40 C92 24,78 14,60 14 Z" stroke="${accent}" stroke-width="2.5" fill="url(#bhf)"/>
    <path d="M60 20 C46 20,34 28,30 42 L28 52 C24 57,22 62,22 68 C22 74,25 79,30 83" stroke="${accent}" stroke-width="1" stroke-opacity="0.25" fill="none"/>
    <path d="M60 20 C74 20,86 28,90 42 L92 52 C96 57,98 62,98 68 C98 74,95 79,90 83" stroke="${accent}" stroke-width="1" stroke-opacity="0.25" fill="none"/>
    <path d="M34 34 C30 26,22 14,14 6 C18 12,22 22,28 30 Z" fill="url(#bhg)"/>
    <path d="M86 34 C90 26,98 14,106 6 C102 12,98 22,92 30 Z" fill="url(#bhg)"/>
    <path d="M28 42 C22 38,18 32,20 26 C22 32,26 36,30 40" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.6"/>
    <path d="M92 42 C98 38,102 32,100 26 C98 32,94 36,90 40" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.6"/>
    <path d="M30 44 C34 40,40 38,48 40" stroke="${accent}" stroke-width="2.5" stroke-linecap="round" fill="none"/>
    <path d="M90 44 C86 40,80 38,72 40" stroke="${accent}" stroke-width="2.5" stroke-linecap="round" fill="none"/>
    <ellipse cx="42" cy="52" rx="10" ry="8" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.3"/>
    <ellipse cx="42" cy="52" rx="6" ry="5" fill="${accent}"/>
    <ellipse cx="42" cy="52" rx="1.8" ry="4.5" fill="black"/>
    <circle cx="39" cy="50" r="1.5" fill="white" opacity="0.7"/>
    <ellipse cx="78" cy="52" rx="10" ry="8" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.3"/>
    <ellipse cx="78" cy="52" rx="6" ry="5" fill="${accent}"/>
    <ellipse cx="78" cy="52" rx="1.8" ry="4.5" fill="black"/>
    <circle cx="75" cy="50" r="1.5" fill="white" opacity="0.7"/>
    <path d="M52 58 L60 66 L68 58" stroke="${accent}" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
    <circle cx="55" cy="64" r="2" stroke="${accent}" stroke-width="1.2" fill="none"/>
    <circle cx="65" cy="64" r="2" stroke="${accent}" stroke-width="1.2" fill="none"/>
    <path d="M32 74 C38 72,48 71,60 72 C72 71,82 72,88 74" stroke="${accent}" stroke-width="2" fill="none"/>
    <path d="M38 74 L36 84 L40 74" fill="${accent}" fill-opacity="0.9" stroke="${accent}" stroke-width="1"/>
    <path d="M50 73 L49 78 L52 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
    <path d="M56 73 L55 79 L58 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
    <path d="M62 73 L61 79 L64 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
    <path d="M68 73 L67 78 L70 73" fill="${accent}" fill-opacity="0.7" stroke="${accent}" stroke-width="0.8"/>
    <path d="M80 74 L82 84 L78 74" fill="${accent}" fill-opacity="0.9" stroke="${accent}" stroke-width="1"/>
    <path d="M34 76 C40 82,50 84,60 84 C70 84,80 82,86 76" stroke="${accent}" stroke-width="1.5" fill="none" stroke-opacity="0.5"/>
    <path d="M50 26 L54 30 L58 26 L62 30 L66 26 L70 30" stroke="${accent}" stroke-width="0.7" stroke-opacity="0.2" fill="none"/>
  </g>
  <!-- Title -->
  <text x="250" y="52" text-anchor="start" font-family="Inter,Segoe UI,system-ui,sans-serif" font-weight="900" font-size="42" letter-spacing="4" fill="url(#btg)">SOL BEAST</text>
  <!-- Tagline -->
  <text x="252" y="78" text-anchor="start" font-family="JetBrains Mono,Fira Code,monospace" font-weight="500" font-size="13" letter-spacing="3" fill="${accent}" opacity="0.7">MEMECOINS SNIPER</text>
  <!-- Decorative line -->
  <line x1="250" y1="86" x2="460" y2="86" stroke="${accent}" stroke-width="1" stroke-opacity="0.3"/>
  <!-- Subtitle -->
  <text x="252" y="102" text-anchor="start" font-family="JetBrains Mono,Fira Code,monospace" font-weight="400" font-size="10" letter-spacing="2" fill="${accent}" opacity="0.4">// ULTRA-FAST SOLANA TOKEN SNIPING</text>
  <!-- Corner accents -->
  <path d="M2 12 L2 2 L12 2" stroke="${accent}" stroke-width="2" fill="none"/>
  <path d="M468 2 L478 2 L478 12" stroke="${accent}" stroke-width="2" fill="none"/>
  <path d="M2 108 L2 118 L12 118" stroke="${accent}" stroke-width="2" fill="none"/>
  <path d="M468 118 L478 118 L478 108" stroke="${accent}" stroke-width="2" fill="none"/>
</svg>`
}

// ─── Helpers ────────────────────────────────────────────────────────
async function ensureDir(dir) {
  if (!existsSync(dir)) await mkdir(dir, { recursive: true })
}

async function svgToPng(svgString, width, height) {
  const { Resvg } = await import('@resvg/resvg-js')
  const opts = {
    fitTo: { mode: 'width', value: width },
    background: 'rgba(0, 0, 0, 1)',
    font: {
      loadSystemFonts: true,
    },
  }
  const resvg = new Resvg(svgString, opts)
  const pngData = resvg.render()
  return pngData.asPng()
}

// ─── Main ───────────────────────────────────────────────────────────
async function main() {
  console.log('🎨 Sol Beast — Brand Asset Generator\n')

  // Ensure deps
  const needed = []
  try { await import('@resvg/resvg-js') } catch { needed.push('@resvg/resvg-js') }
  try { await import('png-to-ico') } catch { needed.push('png-to-ico') }
  if (needed.length) {
    console.log(`📦 Installing: ${needed.join(', ')}`)
    execSync(`npm install --no-save ${needed.join(' ')}`, { cwd: ROOT, stdio: 'inherit' })
  }

  // Create output dirs
  const dirs = [
    join(OUT, 'logo', 'svg'),
    join(OUT, 'logo', 'png'),
    join(OUT, 'banner', 'svg'),
    join(OUT, 'banner', 'png'),
  ]
  for (const d of dirs) await ensureDir(d)

  let totalFiles = 0

  // Generate per-theme assets
  for (const theme of THEMES) {
    const { id, accent } = theme

    // ── Logo SVGs ──
    const lSvg = logoSvg(accent, '0.15', '0.03')
    const logoSvgPath = join(OUT, 'logo', 'svg', `logo-${id}.svg`)
    await writeFile(logoSvgPath, lSvg)
    console.log(`  ✅ logo/svg/logo-${id}.svg`)
    totalFiles++

    // ── Logo PNGs ──
    for (const size of LOGO_SIZES) {
      const png = await svgToPng(lSvg, size, size)
      const pngPath = join(OUT, 'logo', 'png', `logo-${id}-${size}.png`)
      await writeFile(pngPath, png)
      console.log(`  ✅ logo/png/logo-${id}-${size}.png`)
      totalFiles++
    }

    // ── Banner SVGs ──
    const bSvg = bannerSvg(accent, '0.15', '0.03')
    const bannerSvgPath = join(OUT, 'banner', 'svg', `banner-${id}.svg`)
    await writeFile(bannerSvgPath, bSvg)
    console.log(`  ✅ banner/svg/banner-${id}.svg`)
    totalFiles++

    // ── Banner PNGs ──
    const bannerPng = await svgToPng(bSvg, BANNER_WIDTH, BANNER_HEIGHT)
    const bannerPngPath = join(OUT, 'banner', 'png', `banner-${id}-${BANNER_WIDTH}.png`)
    await writeFile(bannerPngPath, bannerPng)
    console.log(`  ✅ banner/png/banner-${id}-${BANNER_WIDTH}.png`)
    totalFiles++
  }

  // ── Favicon ──
  try {
    const pngToIco = (await import('png-to-ico')).default
    const logoPng32 = await svgToPng(logoSvg('#00ff41', '0.15', '0.03'), 32, 32)
    const ico = await pngToIco([logoPng32])
    await writeFile(join(OUT, 'favicon.ico'), ico)
    console.log(`  ✅ favicon.ico`)
    totalFiles++
  } catch (err) {
    // Fallback: just save the 32px PNG as the "favicon"
    console.log(`  ⚠️  png-to-ico failed (${err.message}), saving 32px PNG as fallback`)
    const logoPng32 = await svgToPng(logoSvg('#00ff41', '0.15', '0.03'), 32, 32)
    await writeFile(join(OUT, 'favicon.png'), logoPng32)
    totalFiles++
  }

  // ── Copy static logo.svg to docs/public for VitePress hero ──
  const docsPublic = join(ROOT, 'docs', 'public')
  await ensureDir(docsPublic)
  const defaultLogo = logoSvg('#00ff41', '0.15', '0.03')
  await writeFile(join(docsPublic, 'logo.svg'), defaultLogo)
  console.log(`  ✅ docs/public/logo.svg (VitePress hero fallback)`)
  totalFiles++

  // ── Copy favicon to docs/public and frontend/public ──
  const favicoSrc = join(OUT, 'favicon.ico')
  const favicoPng = join(OUT, 'favicon.png')
  const favicoFile = existsSync(favicoSrc) ? favicoSrc : favicoPng
  if (existsSync(favicoFile)) {
    const favicoData = await readFile(favicoFile)
    const fname = existsSync(favicoSrc) ? 'favicon.ico' : 'favicon.png'

    await writeFile(join(docsPublic, fname), favicoData)
    console.log(`  ✅ docs/public/${fname}`)

    const frontendPublic = join(ROOT, 'frontend', 'public')
    await ensureDir(frontendPublic)
    await writeFile(join(frontendPublic, fname), favicoData)
    console.log(`  ✅ frontend/public/${fname}`)
    totalFiles += 2
  }

  console.log(`\n🎉 Done! Generated ${totalFiles} files in assets/brand/\n`)
}

main().catch(err => {
  console.error('❌ Error:', err)
  process.exit(1)
})
