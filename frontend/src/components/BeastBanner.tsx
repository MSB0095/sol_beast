import { useEffect, useState } from 'react'

/**
 * BeastBanner — Horizontal banner with the beast logo + "SOL BEAST" title + tagline.
 * All SVG so it exports cleanly. Fully theme-aware via CSS variables.
 */
export default function BeastBanner({
  width = 600,
  animated = true,
}: {
  width?: number
  animated?: boolean
}) {
  const [glitch, setGlitch] = useState(false)

  useEffect(() => {
    if (!animated) return
    const interval = setInterval(() => {
      setGlitch(true)
      setTimeout(() => setGlitch(false), 200)
    }, 3000)
    return () => clearInterval(interval)
  }, [animated])

  // Banner aspect ratio is 4:1
  const height = width / 4

  return (
    <div className="relative group" style={{ width, height }}>
      <svg
        viewBox="0 0 480 120"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className={`w-full h-full transition-all duration-300 ${animated ? 'group-hover:scale-[1.02]' : ''}`}
        style={{
          filter: glitch
            ? 'drop-shadow(2px 0 0 #ff0062) drop-shadow(-2px 0 0 #00d9ff)'
            : '',
        }}
      >
        <defs>
          <radialGradient id="bannerEyeGlow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity="1" />
            <stop offset="60%" stopColor="var(--theme-accent)" stopOpacity="0.6" />
            <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity="0" />
          </radialGradient>
          <linearGradient id="bannerHornGrad" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity="1" />
            <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity="0.3" />
          </linearGradient>
          <linearGradient id="bannerHeadFill" x1="50%" y1="0%" x2="50%" y2="100%">
            <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity="0.15" />
            <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity="0.03" />
          </linearGradient>
          <linearGradient id="bannerTextGrad" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity="1" />
            <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity="0.7" />
          </linearGradient>
        </defs>

        {/* ──── Background ──── */}
        <rect width="480" height="120" fill="black" rx="8" />
        {/* Border */}
        <rect
          x="1" y="1" width="478" height="118" rx="7"
          stroke="var(--theme-accent)" strokeWidth="2" fill="none"
          strokeOpacity="0.6"
        />
        {/* Inner accent line */}
        <rect
          x="4" y="4" width="472" height="112" rx="5"
          stroke="var(--theme-accent)" strokeWidth="0.5" fill="none"
          strokeOpacity="0.15"
        />

        {/* ──── Beast Logo (scaled down, positioned left) ──── */}
        <g transform="translate(10, 5) scale(0.92)">
          {/* Head outline */}
          <path
            d="M60 14 C42 14, 28 24, 24 40 L22 52 C18 58, 16 64, 16 70 C16 78, 20 84, 26 88 L26 94 C26 97, 28 100, 32 100 L42 100 L42 106 C42 108, 44 110, 46 110 L74 110 C76 110, 78 108, 78 106 L78 100 L88 100 C92 100, 94 97, 94 94 L94 88 C100 84, 104 78, 104 70 C104 64, 102 58, 98 52 L96 40 C92 24, 78 14, 60 14 Z"
            stroke="var(--theme-accent)" strokeWidth="2.5" fill="url(#bannerHeadFill)"
          />
          {/* Inner contour */}
          <path d="M60 20 C46 20, 34 28, 30 42 L28 52 C24 57, 22 62, 22 68 C22 74, 25 79, 30 83" stroke="var(--theme-accent)" strokeWidth="1" strokeOpacity="0.25" fill="none" />
          <path d="M60 20 C74 20, 86 28, 90 42 L92 52 C96 57, 98 62, 98 68 C98 74, 95 79, 90 83" stroke="var(--theme-accent)" strokeWidth="1" strokeOpacity="0.25" fill="none" />
          {/* Horns */}
          <path d="M34 34 C30 26, 22 14, 14 6 C18 12, 22 22, 28 30 Z" fill="url(#bannerHornGrad)" />
          <path d="M86 34 C90 26, 98 14, 106 6 C102 12, 98 22, 92 30 Z" fill="url(#bannerHornGrad)" />
          {/* Ears */}
          <path d="M28 42 C22 38, 18 32, 20 26 C22 32, 26 36, 30 40" stroke="var(--theme-accent)" strokeWidth="1.5" fill="none" strokeOpacity="0.6" />
          <path d="M92 42 C98 38, 102 32, 100 26 C98 32, 94 36, 90 40" stroke="var(--theme-accent)" strokeWidth="1.5" fill="none" strokeOpacity="0.6" />
          {/* Brows */}
          <path d="M30 44 C34 40, 40 38, 48 40" stroke="var(--theme-accent)" strokeWidth="2.5" strokeLinecap="round" fill="none" />
          <path d="M90 44 C86 40, 80 38, 72 40" stroke="var(--theme-accent)" strokeWidth="2.5" strokeLinecap="round" fill="none" />
          {/* Left eye */}
          <ellipse cx="42" cy="52" rx="10" ry="8" stroke="var(--theme-accent)" strokeWidth="1.5" fill="none" strokeOpacity="0.3" />
          <ellipse cx="42" cy="52" rx="6" ry="5" fill="var(--theme-accent)" />
          <ellipse cx="42" cy="52" rx="1.8" ry="4.5" fill="black" />
          <circle cx="39" cy="50" r="1.5" fill="white" opacity="0.7" />
          {/* Right eye */}
          <ellipse cx="78" cy="52" rx="10" ry="8" stroke="var(--theme-accent)" strokeWidth="1.5" fill="none" strokeOpacity="0.3" />
          <ellipse cx="78" cy="52" rx="6" ry="5" fill="var(--theme-accent)" />
          <ellipse cx="78" cy="52" rx="1.8" ry="4.5" fill="black" />
          <circle cx="75" cy="50" r="1.5" fill="white" opacity="0.7" />
          {/* Muzzle */}
          <path d="M52 58 L60 66 L68 58" stroke="var(--theme-accent)" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
          <circle cx="55" cy="64" r="2" stroke="var(--theme-accent)" strokeWidth="1.2" fill="none" />
          <circle cx="65" cy="64" r="2" stroke="var(--theme-accent)" strokeWidth="1.2" fill="none" />
          {/* Upper jaw + fangs */}
          <path d="M32 74 C38 72, 48 71, 60 72 C72 71, 82 72, 88 74" stroke="var(--theme-accent)" strokeWidth="2" fill="none" />
          <path d="M38 74 L36 84 L40 74" fill="var(--theme-accent)" fillOpacity="0.9" stroke="var(--theme-accent)" strokeWidth="1" />
          <path d="M50 73 L49 78 L52 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
          <path d="M56 73 L55 79 L58 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
          <path d="M62 73 L61 79 L64 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
          <path d="M68 73 L67 78 L70 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
          <path d="M80 74 L82 84 L78 74" fill="var(--theme-accent)" fillOpacity="0.9" stroke="var(--theme-accent)" strokeWidth="1" />
          {/* Lower jaw */}
          <path d="M34 76 C40 82, 50 84, 60 84 C70 84, 80 82, 86 76" stroke="var(--theme-accent)" strokeWidth="1.5" fill="none" strokeOpacity="0.5" />
          {/* Forehead scales */}
          <path d="M50 26 L54 30 L58 26 L62 30 L66 26 L70 30" stroke="var(--theme-accent)" strokeWidth="0.7" strokeOpacity="0.2" fill="none" />
        </g>

        {/* ──── Title: SOL BEAST ──── */}
        <text
          x="250"
          y="52"
          textAnchor="start"
          fontFamily="'Inter', 'Segoe UI', system-ui, sans-serif"
          fontWeight="900"
          fontSize="42"
          letterSpacing="4"
          fill="url(#bannerTextGrad)"
          style={{ textShadow: animated ? undefined : undefined }}
        >
          SOL BEAST
        </text>
        {/* Glitch layers (via duplicate text elements with offset) */}
        {glitch && (
          <>
            <text
              x="252"
              y="52"
              textAnchor="start"
              fontFamily="'Inter', 'Segoe UI', system-ui, sans-serif"
              fontWeight="900"
              fontSize="42"
              letterSpacing="4"
              fill="#ff0062"
              opacity="0.4"
            >
              SOL BEAST
            </text>
            <text
              x="248"
              y="52"
              textAnchor="start"
              fontFamily="'Inter', 'Segoe UI', system-ui, sans-serif"
              fontWeight="900"
              fontSize="42"
              letterSpacing="4"
              fill="#00d9ff"
              opacity="0.4"
            >
              SOL BEAST
            </text>
          </>
        )}

        {/* ──── Tagline ──── */}
        <text
          x="252"
          y="78"
          textAnchor="start"
          fontFamily="'JetBrains Mono', 'Fira Code', monospace"
          fontWeight="500"
          fontSize="13"
          letterSpacing="3"
          fill="var(--theme-accent)"
          opacity="0.7"
        >
          MEMECOINS SNIPER
        </text>

        {/* ──── Decorative line separator ──── */}
        <line
          x1="250"
          y1="86"
          x2="460"
          y2="86"
          stroke="var(--theme-accent)"
          strokeWidth="1"
          strokeOpacity="0.3"
        />

        {/* ──── Version tag ──── */}
        <text
          x="252"
          y="102"
          textAnchor="start"
          fontFamily="'JetBrains Mono', 'Fira Code', monospace"
          fontWeight="400"
          fontSize="10"
          letterSpacing="2"
          fill="var(--theme-accent)"
          opacity="0.4"
        >
          {'// ULTRA-FAST SOLANA TOKEN SNIPING'}
        </text>

        {/* ──── Corner accents ──── */}
        {/* Top-left */}
        <path d="M2 12 L2 2 L12 2" stroke="var(--theme-accent)" strokeWidth="2" fill="none" />
        {/* Top-right */}
        <path d="M468 2 L478 2 L478 12" stroke="var(--theme-accent)" strokeWidth="2" fill="none" />
        {/* Bottom-left */}
        <path d="M2 108 L2 118 L12 118" stroke="var(--theme-accent)" strokeWidth="2" fill="none" />
        {/* Bottom-right */}
        <path d="M468 118 L478 118 L478 108" stroke="var(--theme-accent)" strokeWidth="2" fill="none" />

        {/* ──── Animated scan line ──── */}
        {animated && (
          <rect
            x="0" y="0" width="480" height="2"
            fill="var(--theme-accent)"
            opacity="0.15"
          >
            <animateTransform
              attributeName="transform"
              type="translate"
              values="0 0; 0 120; 0 0"
              dur="4s"
              repeatCount="indefinite"
            />
          </rect>
        )}
      </svg>
    </div>
  )
}
