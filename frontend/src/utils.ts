import type { Mode } from './types'

export const SOL = 1_000_000_000
export const LAMPORTS = SOL

export function lamports(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—'
  return new Intl.NumberFormat('en-US').format(Math.round(value))
}

export function sol(value: number | null | undefined, digits = 4): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—'
  return (value / SOL).toLocaleString('en-US', {
    minimumFractionDigits: Math.min(2, digits),
    maximumFractionDigits: digits,
  })
}

export function signedSol(value: number | null | undefined, digits = 4): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—'
  const text = sol(Math.abs(value), digits)
  if (value > 0) return `+${text}`
  if (value < 0) return `-${text}`
  return text
}

export function compact(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—'
  if (Math.abs(value) < 10_000) return new Intl.NumberFormat('en-US').format(value)
  return Intl.NumberFormat('en-US', {
    notation: 'compact',
    maximumFractionDigits: 2,
  }).format(value)
}

export function pct(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—'
  return `${(value * 100).toFixed(digits)}%`
}

export function bps(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—'
  return `${value.toFixed(0)} bps`
}

export function shortAddress(address: string | null | undefined, head = 4, tail = 4): string {
  if (!address) return '—'
  if (address.length <= head + tail + 2) return address
  return `${address.slice(0, head)}…${address.slice(-tail)}`
}

export function timeAgo(timestamp: number | null | undefined): string {
  if (!timestamp) return 'never'
  const delta = Math.max(0, Date.now() - timestamp)
  if (delta < 1_000) return 'now'
  if (delta < 60_000) return `${Math.floor(delta / 1_000)}s ago`
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`
  return `${Math.floor(delta / 86_400_000)}d ago`
}

export function clockTime(timestamp: number | null | undefined): string {
  if (!timestamp) return '—'
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

export function dateTime(timestamp: number | null | undefined): string {
  if (!timestamp) return '—'
  return new Date(timestamp).toLocaleString([], {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })
}

export function duration(seconds: number | null | undefined): string {
  if (!seconds || seconds < 0) return '0s'
  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3_600)
  const minutes = Math.floor((seconds % 3_600) / 60)
  if (days > 0) return `${days}d ${hours}h`
  if (hours > 0) return `${hours}h ${minutes}m`
  if (minutes > 0) return `${minutes}m ${seconds % 60}s`
  return `${Math.floor(seconds)}s`
}

export function modeRank(mode: string): number {
  return { off: 0, radar: 1, canary: 2, live: 3 }[mode as Mode] ?? 0
}

export function modeLabel(mode: string): string {
  return mode.charAt(0).toUpperCase() + mode.slice(1)
}

export function parseJson<T>(value: string | null | undefined, fallback: T): T {
  if (!value) return fallback
  try {
    return JSON.parse(value) as T
  } catch {
    return fallback
  }
}

export function cx(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(' ')
}

export function outcomeTone(outcome: string): 'good' | 'warn' | 'bad' | 'muted' | 'info' {
  switch (outcome) {
    case 'open':
      return 'info'
    case 'taken_late':
      return 'good'
    case 'taken_next_slot':
      return 'warn'
    case 'taken_same_slot':
      return 'bad'
    case 'never_taken':
    case 'open_expired':
      return 'muted'
    default:
      return 'muted'
  }
}

export function statusTone(status: string): 'good' | 'warn' | 'bad' | 'muted' | 'info' {
  switch (status) {
    case 'landed':
    case 'simulated':
      return 'good'
    case 'submitted':
    case 'preparing':
    case 'simulating':
      return 'info'
    case 'dropped':
    case 'reverted':
    case 'failed':
    case 'simulation_failed':
    case 'submit_failed':
    case 'prepare_failed':
      return 'bad'
    case 'pending':
      return 'warn'
    default:
      return 'muted'
  }
}

export function coverageTone(coverage: string): 'good' | 'warn' | 'muted' | 'info' | 'bad' {
  switch (coverage) {
    case 'modeled':
      return 'good'
    case 'decoded':
      return 'info'
    case 'quote_only':
      return 'info'
    case 'stake_pool':
      return 'good'
    case 'disabled':
      return 'bad'
    default:
      return 'muted'
  }
}

export function coverageLabel(coverage: string): string {
  return coverage.replaceAll('_', ' ')
}

export function copyText(value: string): Promise<void> {
  if (navigator.clipboard?.writeText) return navigator.clipboard.writeText(value)
  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  document.body.removeChild(textarea)
  return Promise.resolve()
}
