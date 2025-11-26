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
        viewBox="0 0 100 100"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className={`transition-all duration-300 ${animated ? 'group-hover:scale-110' : ''} ${glitch ? 'animate-pulse' : ''}`}
        style={{
          filter: `drop-shadow(0 0 ${size / 8}px var(--theme-accent)) ${glitch ? 'drop-shadow(2px 0 0 #ff0062) drop-shadow(-2px 0 0 #00d9ff)' : ''}`,
        }}
      >
        {/* Beast Head Outline */}
        <path
          d="M50 10 C35 10, 25 20, 25 35 L25 45 C20 50, 18 55, 18 62 C18 70, 22 75, 28 78 L28 85 C28 88, 30 90, 33 90 L40 90 L40 95 C40 97, 42 98, 44 98 L56 98 C58 98, 60 97, 60 95 L60 90 L67 90 C70 90, 72 88, 72 85 L72 78 C78 75, 82 70, 82 62 C82 55, 80 50, 75 45 L75 35 C75 20, 65 10, 50 10 Z"
          stroke="var(--theme-accent)"
          strokeWidth="2"
          fill="rgba(0, 255, 65, 0.1)"
          className="transition-all duration-300"
        />
        
        {/* Horns */}
        <path
          d="M30 28 L20 15 L25 25 Z"
          fill="var(--theme-accent)"
          className={animated ? 'animate-pulse' : ''}
        />
        <path
          d="M70 28 L80 15 L75 25 Z"
          fill="var(--theme-accent)"
          className={animated ? 'animate-pulse' : ''}
        />
        
        {/* Eyes with Glow */}
        <circle
          cx="38"
          cy="42"
          r="5"
          fill="var(--theme-accent)"
          className={animated ? 'animate-pulse' : ''}
          style={{
            filter: `drop-shadow(0 0 ${size / 10}px var(--theme-accent))`,
          }}
        />
        <circle
          cx="62"
          cy="42"
          r="5"
          fill="var(--theme-accent)"
          className={animated ? 'animate-pulse' : ''}
          style={{
            filter: `drop-shadow(0 0 ${size / 10}px var(--theme-accent))`,
          }}
        />
        
        {/* Sharp Teeth */}
        <path
          d="M35 60 L38 65 L41 60 L44 65 L47 60 L50 65 L53 60 L56 65 L59 60 L62 65 L65 60"
          stroke="var(--theme-accent)"
          strokeWidth="2"
          fill="none"
          strokeLinecap="round"
        />
        
        {/* Nose/Snout */}
        <path
          d="M45 52 L50 56 L55 52"
          stroke="var(--theme-accent)"
          strokeWidth="2"
          fill="none"
          strokeLinecap="round"
        />
        
        {/* Fierce Eyebrows */}
        <path
          d="M32 35 L43 38"
          stroke="var(--theme-accent)"
          strokeWidth="2"
          strokeLinecap="round"
        />
        <path
          d="M68 35 L57 38"
          stroke="var(--theme-accent)"
          strokeWidth="2"
          strokeLinecap="round"
        />
        
        {/* Energy Lines */}
        {animated && (
          <>
            <line
              x1="25"
              y1="50"
              x2="15"
              y2="50"
              stroke="var(--theme-accent)"
              strokeWidth="2"
              className="animate-pulse"
              style={{ animationDelay: '0s' }}
            />
            <line
              x1="75"
              y1="50"
              x2="85"
              y2="50"
              stroke="var(--theme-accent)"
              strokeWidth="2"
              className="animate-pulse"
              style={{ animationDelay: '0.5s' }}
            />
            <line
              x1="50"
              y1="15"
              x2="50"
              y2="5"
              stroke="var(--theme-accent)"
              strokeWidth="2"
              className="animate-pulse"
              style={{ animationDelay: '1s' }}
            />
          </>
        )}
      </svg>

      {/* Glow Effect Background */}
      {animated && (
        <div
          className="absolute inset-0 -z-10 animate-pulse rounded-full blur-xl opacity-50"
          style={{
            background: `radial-gradient(circle, var(--theme-accent) 0%, transparent 70%)`,
          }}
        />
      )}
    </div>
  )
}
