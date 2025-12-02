import { useSettingsStore } from '../store/settingsStore'
import { useBotStore } from '../store/botStore'
import { useState } from 'react'
import { Save, AlertCircle, CheckCircle } from 'lucide-react'

export default function ConfigurationPanel() {
  const { settings, saving, error, saveSettings, updateSetting } = useSettingsStore()
  const { runningState } = useBotStore()
  const [successMessage, setSuccessMessage] = useState('')

  if (!settings) return <div>Loading settings...</div>

  const isBotStopped = runningState === 'stopped'
  const isBotRunning = runningState === 'running'

  const handleSave = async () => {
    await saveSettings(settings)
    if (isBotRunning) {
      setSuccessMessage('Settings updated! Restart bot for WebSocket/Program changes.')
    } else {
      setSuccessMessage('Settings saved successfully!')
    }
    setTimeout(() => setSuccessMessage(''), 5000)
  }

  const handleChange = <K extends keyof typeof settings>(key: K, value: any) => {
    updateSetting(key, value)
  }

  const sections = [
    {
      title: 'RPC & WebSocket Configuration',
      settings: [
        {
          key: 'solana_ws_urls' as const,
          label: 'Solana WebSocket URLs',
          type: 'textarea',
          help: 'Enter WebSocket URLs separated by newlines'
        },
        {
          key: 'solana_rpc_urls' as const,
          label: 'Solana RPC URLs',
          type: 'textarea',
          help: 'Enter RPC URLs separated by newlines'
        },
      ]
    },
    {
      title: 'Trading Strategy',
      settings: [
        { key: 'tp_percent' as const, label: 'Take Profit %', type: 'number', help: 'Sell when profit reaches this %' },
        { key: 'sl_percent' as const, label: 'Stop Loss %', type: 'number', help: 'Sell when loss reaches this %' },
        { key: 'timeout_secs' as const, label: 'Timeout (seconds)', type: 'number', help: 'Auto-sell after this duration' },
        { key: 'buy_amount' as const, label: 'Buy Amount (SOL)', type: 'number', help: 'SOL spent per buy' },
      ]
    },
    {
      title: 'Safety & Sniping Filters',
      settings: [
        { key: 'enable_safer_sniping' as const, label: 'Enable Safer Sniping', type: 'checkbox' },
        { key: 'min_tokens_threshold' as const, label: 'Min Tokens Threshold', type: 'number' },
        { key: 'max_sol_per_token' as const, label: 'Max SOL per Token', type: 'number' },
        { key: 'slippage_bps' as const, label: 'Slippage (bps)', type: 'number', help: '500 = 5%' },
        { key: 'min_liquidity_sol' as const, label: 'Min Liquidity (SOL)', type: 'number' },
        { key: 'max_liquidity_sol' as const, label: 'Max Liquidity (SOL)', type: 'number' },
      ]
    },
    {
      title: 'Position Management',
      settings: [
        { key: 'max_holded_coins' as const, label: 'Max Holdings', type: 'number' },
        { key: 'max_create_to_buy_secs' as const, label: 'Max Create to Buy (secs)', type: 'number' },
      ]
    },
    {
      title: 'Helius Sender Configuration',
      settings: [
        { key: 'helius_sender_enabled' as const, label: 'Enable Helius Sender', type: 'checkbox' },
        { key: 'helius_min_tip_sol' as const, label: 'Min Tip (SOL)', type: 'number' },
        { key: 'helius_priority_fee_multiplier' as const, label: 'Priority Fee Multiplier', type: 'number' },
        { key: 'helius_use_swqos_only' as const, label: 'Use SWQOS Only', type: 'checkbox' },
        { key: 'helius_use_dynamic_tips' as const, label: 'Use Dynamic Tips', type: 'checkbox' },
        { key: 'helius_confirm_timeout_secs' as const, label: 'Confirm Timeout (secs)', type: 'number' },
      ]
    },
    {
      title: 'Advanced Configuration',
      settings: [
        { key: 'price_source' as const, label: 'Price Source', type: 'select', options: ['wss', 'rpc', 'hybrid'] },
        { key: 'rotate_rpc' as const, label: 'Rotate RPC', type: 'checkbox' },
        { key: 'rpc_rotate_interval_secs' as const, label: 'RPC Rotate Interval (secs)', type: 'number' },
        { key: 'max_subs_per_wss' as const, label: 'Max Subs per WSS', type: 'number' },
        { key: 'sub_ttl_secs' as const, label: 'Subscription TTL (secs)', type: 'number' },
        { key: 'cache_capacity' as const, label: 'Cache Capacity', type: 'number' },
        { key: 'price_cache_ttl_secs' as const, label: 'Price Cache TTL (secs)', type: 'number' },
      ]
    }
  ]

  return (
    <div className="space-y-8">
      {isBotRunning && (
        <div className="alert-warning rounded-xl p-5 flex gap-3 relative overflow-hidden animate-fade-in-up">
          <AlertCircle size={24} className="flex-shrink-0 mt-0.5 animate-pulse" />
          <div>
            <p className="font-bold uppercase tracking-widest text-sm mb-1">BOT IS RUNNING</p>
            <p className="text-sm opacity-90">
              You can update settings while running. Trading parameters (TP, SL, buy amount, etc.) apply to future trades.
              <br />
              <span className="font-semibold">⚠️ WebSocket URL and Program ID changes require bot restart.</span>
            </p>
          </div>
        </div>
      )}

      {error && (
        <div className="alert-error rounded-xl p-5 flex gap-3 relative overflow-hidden animate-fade-in-up">
          <AlertCircle size={24} className="flex-shrink-0 mt-0.5 animate-pulse" />
          <div>
            <p className="font-bold uppercase tracking-widest text-sm mb-1">SYSTEM ERROR</p>
            <p className="text-sm opacity-90">{error}</p>
          </div>
        </div>
      )}

      {successMessage && (
        <div className="alert-success rounded-xl p-5 flex gap-3 relative overflow-hidden animate-fade-in-up">
          <CheckCircle size={24} className="flex-shrink-0 mt-0.5" />
          <p className="uppercase tracking-widest font-bold text-sm">{successMessage}</p>
        </div>
      )}

      {sections.map((section, idx) => (
        <div key={idx} className="glass-card rounded-2xl p-6 animate-fade-in-up" style={{ 
          animationDelay: `${idx * 0.1}s`
        }}>
          <h3 className="text-xl font-black mb-6 glow-text uppercase tracking-wider flex items-center gap-3">
            <span className="w-2 h-2 bg-[var(--theme-accent)] rounded-full animate-pulse" style={{ boxShadow: '0 0 10px var(--theme-accent)' }}></span>
            {section.title}
          </h3>
          
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {section.settings.map((setting) => (
              <div key={setting.key}>
                <label className="block text-sm font-medium mb-2">
                  {setting.label}
                  {(setting as any).help && <span className="text-xs ml-2" style={{ color: 'var(--theme-text-muted)' }}>({(setting as any).help})</span>}
                </label>

                {setting.type === 'checkbox' && (
                  <div className="flex items-center gap-3">
                    <input
                      type="checkbox"
                      checked={settings[setting.key] as any}
                      onChange={(e) => handleChange(setting.key, e.target.checked)}
                      className="w-5 h-5 rounded cursor-pointer accent-[var(--theme-accent)]"
                      style={{
                        backgroundColor: 'var(--theme-bg-input)',
                        borderColor: 'var(--theme-input-border)'
                      }}
                    />
                    <span style={{ color: 'var(--theme-text-secondary)' }} className="text-sm">
                      {settings[setting.key] ? 'ENABLED' : 'DISABLED'}
                    </span>
                  </div>
                )}

                {setting.type === 'number' && (
                  <input
                    type="number"
                    value={settings[setting.key] as any}
                    onChange={(e) => handleChange(setting.key, parseFloat(e.target.value))}
                    className="w-full px-3 py-2 rounded-xl transition-all"
                    step="any"
                  />
                )}

                {setting.type === 'textarea' && (
                  <textarea
                    value={(settings[setting.key] as string[]).join('\n')}
                    onChange={(e) => handleChange(setting.key, e.target.value.split('\n').filter(Boolean))}
                    className="w-full px-3 py-2 rounded-xl min-h-20 text-sm transition-all"
                  />
                )}

                {setting.type === 'select' && (
                  <select
                    value={settings[setting.key] as string}
                    onChange={(e) => handleChange(setting.key, e.target.value)}
                    className="w-full px-3 py-2 rounded-xl transition-all uppercase"
                  >
                    {(setting as any).options?.map((opt: string) => (
                      <option key={opt} value={opt} style={{ 
                        backgroundColor: 'var(--theme-bg-input)',
                        color: 'var(--theme-input-text)'
                      }}>
                        {opt.toUpperCase()}
                      </option>
                    ))}
                  </select>
                )}
              </div>
            ))}
          </div>
        </div>
      ))}

      {/* Save Button */}
      <div className="flex justify-end gap-3 sticky bottom-0 py-4 px-6 rounded-xl" style={{
        backgroundColor: 'var(--theme-bg-card)',
        borderTop: '2px solid var(--theme-accent)',
        boxShadow: '0 -5px 20px var(--glow-color)'
      }}>
        <button
          onClick={handleSave}
          disabled={saving}
          className="flex items-center gap-2 px-6 py-3 rounded-xl disabled:opacity-50 transition-all hover:scale-105"
        >
          <Save size={18} />
          {saving ? 'SAVING...' : 'SAVE SETTINGS'}
        </button>
      </div>
    </div>
  )
}
