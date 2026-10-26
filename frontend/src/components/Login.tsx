import { useState, type FormEvent } from 'react'
import {
  ArrowRight,
  Eye,
  EyeOff,
  Fingerprint,
  KeyRound,
  LockKeyhole,
  ShieldCheck,
  TerminalSquare,
  Wifi,
} from 'lucide-react'
import { api, ApiError } from '../api'
import { Button } from './ui'

export function Login({ onLogin }: { onLogin: (token: string) => void }) {
  const [token, setToken] = useState('')
  const [show, setShow] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const trimmed = token.trim()
    if (trimmed.length < 24) {
      setError('API tokens are at least 24 characters. Use the admin or read token from secrets.toml.')
      return
    }
    setBusy(true)
    setError(null)
    try {
      await api.status(trimmed)
      onLogin(trimmed)
    } catch (caught) {
      if (caught instanceof ApiError && caught.status === 401) {
        setError('That token was rejected. Check secrets.toml and try again.')
      } else {
        setError(caught instanceof Error ? caught.message : 'cannot reach the local API')
      }
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="login-shell">
      <div className="login-grid" />
      <div className="login-orb login-orb-a" />
      <div className="login-orb login-orb-b" />
      <div className="login-wrap">
        <div className="login-brand">
          <div className="brand-mark brand-mark-lg">
            <Fingerprint size={34} strokeWidth={1.4} />
          </div>
          <div>
            <div className="eyebrow">SOLANA / GREEN-OR-REVERT</div>
            <h1 className="login-title">
              SOL <span>BEAST</span>
            </h1>
            <p className="login-tag">Keeper & liquidity engine command center</p>
          </div>
        </div>

        <div className="login-panel">
          <div className="login-panel-head">
            <div className="login-lock">
              <LockKeyhole size={18} />
            </div>
            <div>
              <h2>Authenticate the operator</h2>
              <p>The API is loopback-only and requires a bearer token. Tokens never leave this tab.</p>
            </div>
          </div>

          <form onSubmit={submit} className="login-form">
            <label className="field">
              <span className="field-label">Admin or read-only token</span>
              <div className="token-wrap">
                <KeyRound size={16} className="token-icon" />
                <input
                  className="input token-input"
                  type={show ? 'text' : 'password'}
                  value={token}
                  onChange={(event) => setToken(event.target.value)}
                  placeholder="paste token from secrets.toml"
                  autoFocus
                  autoComplete="off"
                  spellCheck={false}
                />
                <button
                  type="button"
                  className="token-eye"
                  onClick={() => setShow((value) => !value)}
                  aria-label={show ? 'Hide token' : 'Show token'}
                >
                  {show ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
              </div>
            </label>
            {error && <div className="form-error">{error}</div>}
            <Button type="submit" variant="primary" size="md" className="login-submit" loading={busy}>
              Enter command center
              {!busy && <ArrowRight size={16} />}
            </Button>
          </form>

          <div className="login-steps">
            <div>
              <TerminalSquare size={15} />
              <span>
                <strong>beast run</strong> starts the engine and local API
              </span>
            </div>
            <div>
              <ShieldCheck size={15} />
              <span>
                Tokens are generated in <code>secrets.toml</code> (0600)
              </span>
            </div>
            <div>
              <Wifi size={15} />
              <span>
                API proxy <code>/v1</code> and <code>/healthz</code> → <code>127.0.0.1:8081</code>
              </span>
            </div>
          </div>
        </div>

        <div className="login-foot">
          <span>READ / WRITE SEPARATION</span>
          <span>LOOPBACK BIND</span>
          <span>NO CUSTODY</span>
        </div>
      </div>
    </div>
  )
}
