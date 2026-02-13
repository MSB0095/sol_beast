import { useEffect, useState } from 'react'

export default function BeastLogo({ size = 48, animated = true }: { size?: number; animated?: boolean }) {
  const [glitch, setGlitch] = useState(false)

  useEffect(() => {
    if (!animated) return
    const interval = setInterval(() => {
      setGlitch(true)
      setTimeout(() => setGlitch(false), 200)
    }, 3000)
    return () => clearInterval(interval)
  }, [animated])

  return (
    <div className="relative group" style={{ width: size, height: size }}>
      {/* Main Beast SVG */}
      <svg
        viewBox="0 0 120 120"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className={`transition-all duration-300 ${animated ? 'group-hover:scale-110' : ''} ${glitch ? 'animate-pulse' : ''}`}
        style={{
          filter: `drop-shadow(0 0 ${size / 8}px var(--theme-accent)) ${glitch ? 'drop-shadow(2px 0 0 #ff0062) drop-shadow(-2px 0 0 #00d9ff)' : ''}`,
        }}
      >
        <defs>
          {/* Radial glow for eyes */}
          <radialGradient id="eyeGlow" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity="1" />
            <stop offset="60%" stopColor="var(--theme-accent)" stopOpacity="0.6" />
            <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity="0" />
          </radialGradient>
          {/* Linear gradient for horns */}
          <linearGradient id="hornGrad" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity="1" />
            <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity="0.3" />
          </linearGradient>
          {/* Accent fill with low opacity */}
          <linearGradient id="headFill" x1="50%" y1="0%" x2="50%" y2="100%">
            <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity="0.15" />
            <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity="0.03" />
          </linearGradient>
        </defs>

        {/* ──── Outer skull / head shape ──── */}
        <path
          d="M60 14
             C42 14, 28 24, 24 40
             L22 52
             C18 58, 16 64, 16 70
             C16 78, 20 84, 26 88
             L26 94 C26 97, 28 100, 32 100
             L42 100 L42 106 C42 108, 44 110, 46 110
             L74 110 C76 110, 78 108, 78 106
             L78 100 L88 100 C92 100, 94 97, 94 94
             L94 88 C100 84, 104 78, 104 70
             C104 64, 102 58, 98 52
             L96 40 C92 24, 78 14, 60 14 Z"
          stroke="var(--theme-accent)"
          strokeWidth="2.5"
          fill="url(#headFill)"
          className="transition-all duration-300"
        />
        {/* Inner skull contour line for depth */}
        <path
          d="M60 20
             C46 20, 34 28, 30 42
             L28 52
             C24 57, 22 62, 22 68
             C22 74, 25 79, 30 83"
          stroke="var(--theme-accent)"
          strokeWidth="1"
          strokeOpacity="0.25"
          fill="none"
        />
        <path
          d="M60 20
             C74 20, 86 28, 90 42
             L92 52
             C96 57, 98 62, 98 68
             C98 74, 95 79, 90 83"
          stroke="var(--theme-accent)"
          strokeWidth="1"
          strokeOpacity="0.25"
          fill="none"
        />

        {/* ──── Left horn (curved, segmented) ──── */}
        <path
          d="M34 34 C30 26, 22 14, 14 6 C18 12, 22 22, 28 30 Z"
          fill="url(#hornGrad)"
          className={animated ? 'animate-pulse' : ''}
        />
        {/* Horn ridge lines */}
        <line x1="28" y1="28" x2="20" y2="14" stroke="var(--theme-accent)" strokeWidth="0.8" strokeOpacity="0.4" />
        <line x1="30" y1="30" x2="24" y2="20" stroke="var(--theme-accent)" strokeWidth="0.6" strokeOpacity="0.3" />

        {/* ──── Right horn (curved, segmented) ──── */}
        <path
          d="M86 34 C90 26, 98 14, 106 6 C102 12, 98 22, 92 30 Z"
          fill="url(#hornGrad)"
          className={animated ? 'animate-pulse' : ''}
        />
        <line x1="92" y1="28" x2="100" y2="14" stroke="var(--theme-accent)" strokeWidth="0.8" strokeOpacity="0.4" />
        <line x1="90" y1="30" x2="96" y2="20" stroke="var(--theme-accent)" strokeWidth="0.6" strokeOpacity="0.3" />

        {/* ──── Ear / crest left ──── */}
        <path
          d="M28 42 C22 38, 18 32, 20 26 C22 32, 26 36, 30 40"
          stroke="var(--theme-accent)"
          strokeWidth="1.5"
          fill="none"
          strokeOpacity="0.6"
        />
        {/* ──── Ear / crest right ──── */}
        <path
          d="M92 42 C98 38, 102 32, 100 26 C98 32, 94 36, 90 40"
          stroke="var(--theme-accent)"
          strokeWidth="1.5"
          fill="none"
          strokeOpacity="0.6"
        />

        {/* ──── Brow ridge left ──── */}
        <path
          d="M30 44 C34 40, 40 38, 48 40"
          stroke="var(--theme-accent)"
          strokeWidth="2.5"
          strokeLinecap="round"
          fill="none"
        />
        {/* ──── Brow ridge right ──── */}
        <path
          d="M90 44 C86 40, 80 38, 72 40"
          stroke="var(--theme-accent)"
          strokeWidth="2.5"
          strokeLinecap="round"
          fill="none"
        />

        {/* ──── Left eye socket ──── */}
        <ellipse cx="42" cy="52" rx="10" ry="8" stroke="var(--theme-accent)" strokeWidth="1.5" fill="none" strokeOpacity="0.3" />
        {/* Eye glow aura */}
        <ellipse cx="42" cy="52" rx="8" ry="6" fill="url(#eyeGlow)" opacity="0.3" className={animated ? 'animate-pulse' : ''} />
        {/* Eye iris */}
        <ellipse
          cx="42" cy="52" rx="6" ry="5"
          fill="var(--theme-accent)"
          className={animated ? 'animate-pulse' : ''}
          style={{ filter: `drop-shadow(0 0 ${size / 6}px var(--theme-accent))` }}
        />
        {/* Slit pupil */}
        <ellipse cx="42" cy="52" rx="1.8" ry="4.5" fill="black" />
        {/* Eye highlight */}
        <circle cx="39" cy="50" r="1.5" fill="white" opacity="0.7" />

        {/* ──── Right eye socket ──── */}
        <ellipse cx="78" cy="52" rx="10" ry="8" stroke="var(--theme-accent)" strokeWidth="1.5" fill="none" strokeOpacity="0.3" />
        <ellipse cx="78" cy="52" rx="8" ry="6" fill="url(#eyeGlow)" opacity="0.3" className={animated ? 'animate-pulse' : ''} />
        <ellipse
          cx="78" cy="52" rx="6" ry="5"
          fill="var(--theme-accent)"
          className={animated ? 'animate-pulse' : ''}
          style={{ filter: `drop-shadow(0 0 ${size / 6}px var(--theme-accent))` }}
        />
        <ellipse cx="78" cy="52" rx="1.8" ry="4.5" fill="black" />
        <circle cx="75" cy="50" r="1.5" fill="white" opacity="0.7" />

        {/* ──── Muzzle bridge ──── */}
        <path
          d="M52 58 L60 66 L68 58"
          stroke="var(--theme-accent)"
          strokeWidth="2"
          fill="none"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        {/* Nostrils */}
        <circle cx="55" cy="64" r="2" stroke="var(--theme-accent)" strokeWidth="1.2" fill="none" />
        <circle cx="65" cy="64" r="2" stroke="var(--theme-accent)" strokeWidth="1.2" fill="none" />

        {/* ──── Cheek texture / scales ──── */}
        <path d="M28 60 C30 58, 34 56, 32 62" stroke="var(--theme-accent)" strokeWidth="0.8" strokeOpacity="0.3" fill="none" />
        <path d="M30 66 C32 64, 36 62, 34 68" stroke="var(--theme-accent)" strokeWidth="0.8" strokeOpacity="0.3" fill="none" />
        <path d="M92 60 C90 58, 86 56, 88 62" stroke="var(--theme-accent)" strokeWidth="0.8" strokeOpacity="0.3" fill="none" />
        <path d="M90 66 C88 64, 84 62, 86 68" stroke="var(--theme-accent)" strokeWidth="0.8" strokeOpacity="0.3" fill="none" />

        {/* ──── Upper jaw line ──── */}
        <path
          d="M32 74 C38 72, 48 71, 60 72 C72 71, 82 72, 88 74"
          stroke="var(--theme-accent)"
          strokeWidth="2"
          fill="none"
        />

        {/* ──── Upper fangs (large, individual) ──── */}
        {/* Left fang */}
        <path
          d="M38 74 L36 84 L40 74"
          fill="var(--theme-accent)"
          fillOpacity="0.9"
          stroke="var(--theme-accent)"
          strokeWidth="1"
        />
        {/* Left inner teeth */}
        <path d="M44 74 L43 80 L46 74" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
        <path d="M50 73 L49 78 L52 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
        {/* Center teeth */}
        <path d="M56 73 L55 79 L58 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
        <path d="M62 73 L61 79 L64 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
        {/* Right inner teeth */}
        <path d="M68 73 L67 78 L70 73" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
        <path d="M74 74 L73 80 L76 74" fill="var(--theme-accent)" fillOpacity="0.7" stroke="var(--theme-accent)" strokeWidth="0.8" />
        {/* Right fang */}
        <path
          d="M80 74 L82 84 L78 74"
          fill="var(--theme-accent)"
          fillOpacity="0.9"
          stroke="var(--theme-accent)"
          strokeWidth="1"
        />

        {/* ──── Lower jaw ──── */}
        <path
          d="M34 76 C40 82, 50 84, 60 84 C70 84, 80 82, 86 76"
          stroke="var(--theme-accent)"
          strokeWidth="1.5"
          fill="none"
          strokeOpacity="0.5"
        />
        {/* Lower teeth (pointing up) */}
        <path d="M42 84 L44 78 L46 84" fill="var(--theme-accent)" fillOpacity="0.5" stroke="var(--theme-accent)" strokeWidth="0.6" />
        <path d="M52 84 L54 79 L56 84" fill="var(--theme-accent)" fillOpacity="0.5" stroke="var(--theme-accent)" strokeWidth="0.6" />
        <path d="M62 84 L64 79 L66 84" fill="var(--theme-accent)" fillOpacity="0.5" stroke="var(--theme-accent)" strokeWidth="0.6" />
        <path d="M72 84 L74 78 L76 84" fill="var(--theme-accent)" fillOpacity="0.5" stroke="var(--theme-accent)" strokeWidth="0.6" />

        {/* ──── Forehead texture / scales ──── */}
        <path d="M50 26 L54 30 L58 26 L62 30 L66 26 L70 30" stroke="var(--theme-accent)" strokeWidth="0.7" strokeOpacity="0.2" fill="none" />
        <path d="M46 32 L50 36 L54 32 L58 36 L62 32 L66 36 L70 32 L74 36" stroke="var(--theme-accent)" strokeWidth="0.7" strokeOpacity="0.15" fill="none" />

        {/* ──── Side jaw ridges ──── */}
        <path d="M24 70 L28 74 L26 78" stroke="var(--theme-accent)" strokeWidth="1.2" strokeOpacity="0.4" fill="none" />
        <path d="M96 70 L92 74 L94 78" stroke="var(--theme-accent)" strokeWidth="1.2" strokeOpacity="0.4" fill="none" />

        {/* ──── Energy lines (animated) ──── */}
        {animated && (
          <>
            {/* Left energy burst */}
            <line x1="18" y1="52" x2="6" y2="48" stroke="var(--theme-accent)" strokeWidth="2" className="animate-pulse" style={{ animationDelay: '0s' }} />
            <line x1="18" y1="58" x2="8" y2="62" stroke="var(--theme-accent)" strokeWidth="1.5" strokeOpacity="0.6" className="animate-pulse" style={{ animationDelay: '0.2s' }} />
            {/* Right energy burst */}
            <line x1="102" y1="52" x2="114" y2="48" stroke="var(--theme-accent)" strokeWidth="2" className="animate-pulse" style={{ animationDelay: '0.5s' }} />
            <line x1="102" y1="58" x2="112" y2="62" stroke="var(--theme-accent)" strokeWidth="1.5" strokeOpacity="0.6" className="animate-pulse" style={{ animationDelay: '0.7s' }} />
            {/* Top energy */}
            <line x1="60" y1="12" x2="60" y2="2" stroke="var(--theme-accent)" strokeWidth="2" className="animate-pulse" style={{ animationDelay: '1s' }} />
            <line x1="52" y1="14" x2="48" y2="4" stroke="var(--theme-accent)" strokeWidth="1.2" strokeOpacity="0.5" className="animate-pulse" style={{ animationDelay: '1.2s' }} />
            <line x1="68" y1="14" x2="72" y2="4" stroke="var(--theme-accent)" strokeWidth="1.2" strokeOpacity="0.5" className="animate-pulse" style={{ animationDelay: '1.4s' }} />
          </>
        )}
      </svg>

      {/* Glow Effect Background */}
      {animated && (
        <div
          className="absolute inset-0 -z-10 animate-pulse rounded-full blur-xl opacity-40"
          style={{
            background: `radial-gradient(circle, var(--theme-accent) 0%, transparent 70%)`,
          }}
        />
      )}
    </div>
  )
}
