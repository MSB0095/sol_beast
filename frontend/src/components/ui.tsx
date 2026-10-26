import {
  useEffect,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type TextareaHTMLAttributes,
} from 'react'
import { Check, Copy, Loader2, X } from 'lucide-react'
import { copyText, cx } from '../utils'

export type Tone = 'good' | 'warn' | 'bad' | 'muted' | 'info' | 'accent'

const toneClasses: Record<Tone, string> = {
  good: 'badge-good',
  warn: 'badge-warn',
  bad: 'badge-bad',
  muted: 'badge-muted',
  info: 'badge-info',
  accent: 'badge-accent',
}

export function Badge({
  children,
  tone = 'muted',
  dot = false,
  className,
}: {
  children: ReactNode
  tone?: Tone
  dot?: boolean
  className?: string
}) {
  return (
    <span className={cx('badge', toneClasses[tone], className)}>
      {dot && <span className="badge-dot" />}
      {children}
    </span>
  )
}

export function Button({
  children,
  variant = 'secondary',
  size = 'md',
  loading = false,
  className,
  disabled,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'outline'
  size?: 'sm' | 'md' | 'icon'
  loading?: boolean
}) {
  return (
    <button
      {...props}
      disabled={disabled || loading}
      className={cx('btn', `btn-${variant}`, `btn-${size}`, className)}
    >
      {loading && <Loader2 size={14} className="animate-spin" />}
      {children}
    </button>
  )
}

export function Panel({
  children,
  className,
  glow = false,
}: {
  children: ReactNode
  className?: string
  glow?: boolean
}) {
  return <section className={cx('panel', glow && 'panel-glow', className)}>{children}</section>
}

export function PanelHeader({
  title,
  subtitle,
  icon,
  actions,
  className,
}: {
  title: ReactNode
  subtitle?: ReactNode
  icon?: ReactNode
  actions?: ReactNode
  className?: string
}) {
  return (
    <div className={cx('panel-header', className)}>
      <div className="panel-heading">
        {icon && <span className="panel-icon">{icon}</span>}
        <div>
          <h2 className="panel-title">{title}</h2>
          {subtitle && <p className="panel-subtitle">{subtitle}</p>}
        </div>
      </div>
      {actions && <div className="panel-actions">{actions}</div>}
    </div>
  )
}

export function StatCard({
  label,
  value,
  detail,
  icon,
  tone = 'accent',
  mono = false,
  className,
}: {
  label: string
  value: ReactNode
  detail?: ReactNode
  icon?: ReactNode
  tone?: Tone
  mono?: boolean
  className?: string
}) {
  return (
    <div className={cx('stat-card', className)}>
      <div className="stat-top">
        <span className="stat-label">{label}</span>
        {icon && <span className={cx('stat-icon', `tone-${tone}`)}>{icon}</span>}
      </div>
      <div className={cx('stat-value', mono && 'font-mono-tech', `tone-${tone}`)}>{value}</div>
      {detail && <div className="stat-detail">{detail}</div>}
    </div>
  )
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  compact = false,
}: {
  icon: ReactNode
  title: string
  description: ReactNode
  action?: ReactNode
  compact?: boolean
}) {
  return (
    <div className={cx('empty-state', compact && 'empty-state-compact')}>
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      <p>{description}</p>
      {action && <div className="empty-action">{action}</div>}
    </div>
  )
}

export function Spinner({ label = 'Loading' }: { label?: string }) {
  return (
    <div className="spinner-row">
      <Loader2 size={16} className="animate-spin text-[var(--accent)]" />
      <span>{label}</span>
    </div>
  )
}

export function Modal({
  open,
  onClose,
  title,
  subtitle,
  children,
  footer,
  wide = false,
}: {
  open: boolean
  onClose: () => void
  title: ReactNode
  subtitle?: ReactNode
  children: ReactNode
  footer?: ReactNode
  wide?: boolean
}) {
  useEffect(() => {
    if (!open) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onClose])

  if (!open) return null
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div
        className={cx('modal', wide && 'modal-wide')}
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <div className="modal-header">
          <div>
            <h2>{title}</h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
          <button className="icon-btn" onClick={onClose} aria-label="Close">
            <X size={18} />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-footer">{footer}</div>}
      </div>
    </div>
  )
}

export function Input({
  label,
  hint,
  error,
  className,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & {
  label?: string
  hint?: string
  error?: string
}) {
  return (
    <label className={cx('field', className)}>
      {label && <span className="field-label">{label}</span>}
      <input {...props} className="input" />
      {hint && !error && <span className="field-hint">{hint}</span>}
      {error && <span className="field-error">{error}</span>}
    </label>
  )
}

export function TextArea({
  label,
  hint,
  className,
  ...props
}: TextareaHTMLAttributes<HTMLTextAreaElement> & { label?: string; hint?: string }) {
  return (
    <label className={cx('field', className)}>
      {label && <span className="field-label">{label}</span>}
      <textarea {...props} className="input input-area" />
      {hint && <span className="field-hint">{hint}</span>}
    </label>
  )
}

export function Select({
  label,
  children,
  className,
  ...props
}: React.SelectHTMLAttributes<HTMLSelectElement> & { label?: string }) {
  return (
    <label className={cx('field', className)}>
      {label && <span className="field-label">{label}</span>}
      <select {...props} className="input input-select">
        {children}
      </select>
    </label>
  )
}

export function CopyButton({ value, label }: { value: string; label?: string }) {
  return (
    <button
      type="button"
      className="copy-btn"
      onClick={(event) => {
        event.stopPropagation()
        void copyText(value)
        const target = event.currentTarget
        target.dataset.copied = 'true'
        window.setTimeout(() => {
          delete target.dataset.copied
        }, 900)
      }}
      title="Copy"
    >
      <Check size={12} className="copy-check" />
      <Copy size={12} className="copy-icon" />
      {label && <span>{label}</span>}
    </button>
  )
}

export function MiniBar({ value, tone = 'accent' }: { value: number; tone?: Tone }) {
  return (
    <div className="mini-bar">
      <span
        className={cx('mini-bar-fill', `tone-bg-${tone}`)}
        style={{ width: `${Math.max(0, Math.min(100, value))}%` }}
      />
    </div>
  )
}

export function KeyValue({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="kv">
      <span>{label}</span>
      <strong>{children}</strong>
    </div>
  )
}
