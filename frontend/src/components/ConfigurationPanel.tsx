import { useSettingsStore, TpLevel, SlLevel } from '../store/settingsStore'
import { useBotStore } from '../store/botStore'
import { useState } from 'react'
import { Save, AlertCircle, CheckCircle, Plus, Trash2 } from 'lucide-react'

export default function ConfigurationPanel() {
  const { settings, saving, error, saveSettings, updateSetting } = useSettingsStore()
  const { runningState } = useBotStore()
  const [successMessage, setSuccessMessage] = useState('')

  if (!settings) return <div>Loading settings...</div>

  const isBotStopped = runningState === 'stopped'

  const handleSave = async () => {
    if (!isBotStopped) {
      return // Don't save if bot is running
    }
    await saveSettings(settings)
    setSuccessMessage('Settings saved successfully!')
    setTimeout(() => setSuccessMessage(''), 3000)
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
      {!isBotStopped && (
        <div className="alert-warning rounded-xl p-5 flex gap-3 relative overflow-hidden animate-fade-in-up">
          <AlertCircle size={24} className="flex-shrink-0 mt-0.5 animate-pulse" />
          <div>
            <p className="font-bold uppercase tracking-widest text-sm mb-1">BOT IS RUNNING</p>
            <p className="text-sm opacity-90">Stop the bot before saving configuration changes</p>
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

      {sections.map((section, idx) => {
        // Insert the multi-level TP/SL panel after the first section ("RPC & WebSocket Configuration")
        const showTpSlAfter = idx === 0;
        
        return (
        <div key={idx}>
        <div className="glass-card rounded-2xl p-6 animate-fade-in-up" style={{ 
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

        {/* Multi-level TP/SL configuration panel — rendered after the first section */}
        {showTpSlAfter && (
          <div className="glass-card rounded-2xl p-6 animate-fade-in-up mt-8" style={{ animationDelay: '0.15s' }}>
            <h3 className="text-xl font-black mb-6 glow-text uppercase tracking-wider flex items-center gap-3">
              <span className="w-2 h-2 bg-[var(--theme-accent)] rounded-full animate-pulse" style={{ boxShadow: '0 0 10px var(--theme-accent)' }}></span>
              Take Profit Levels
            </h3>
            {settings.tp_levels.map((level: TpLevel, i: number) => {
              const tpSum = settings.tp_levels.reduce((s: number, l: TpLevel) => s + l.sell_percent, 0);
              return (
                <div key={i} className="flex items-center gap-3 mb-3">
                  <span className="text-sm font-bold w-10" style={{ color: 'var(--theme-text-secondary)' }}>TP{i + 1}</span>
                  <div className="flex-1">
                    <label className="text-xs mb-1 block" style={{ color: 'var(--theme-text-muted)' }}>Trigger %</label>
                    <input
                      type="number"
                      value={level.trigger_percent}
                      onChange={(e) => {
                        const newLevels = [...settings.tp_levels];
                        newLevels[i] = { ...newLevels[i], trigger_percent: parseFloat(e.target.value) || 0 };
                        handleChange('tp_levels', newLevels);
                      }}
                      className="w-full px-3 py-2 rounded-xl transition-all"
                      step="any"
                    />
                  </div>
                  <div className="flex-1">
                    <label className="text-xs mb-1 block" style={{ color: 'var(--theme-text-muted)' }}>Sell %</label>
                    <input
                      type="number"
                      value={level.sell_percent}
                      onChange={(e) => {
                        const newLevels = [...settings.tp_levels];
                        newLevels[i] = { ...newLevels[i], sell_percent: parseFloat(e.target.value) || 0 };
                        handleChange('tp_levels', newLevels);
                      }}
                      className="w-full px-3 py-2 rounded-xl transition-all"
                      step="any"
                      min="0"
                      max="100"
                    />
                  </div>
                  <button
                    onClick={() => {
                      if (settings.tp_levels.length > 1) {
                        const newLevels = settings.tp_levels.filter((_: TpLevel, j: number) => j !== i);
                        handleChange('tp_levels', newLevels);
                      }
                    }}
                    disabled={settings.tp_levels.length <= 1}
                    className="mt-5 p-2 rounded-lg disabled:opacity-30 hover:opacity-80 transition-all"
                    title="Remove level"
                  >
                    <Trash2 size={16} />
                  </button>
                  {i === settings.tp_levels.length - 1 && tpSum > 100 && (
                    <span className="text-xs mt-5" style={{ color: 'var(--theme-error, #ef4444)' }}>Sum: {tpSum.toFixed(0)}% (&gt;100%)</span>
                  )}
                </div>
              );
            })}
            <div className="flex items-center gap-3 mt-2">
              <button
                onClick={() => {
                  if (settings.tp_levels.length < 4) {
                    handleChange('tp_levels', [...settings.tp_levels, { trigger_percent: 50, sell_percent: 25 }]);
                  }
                }}
                disabled={settings.tp_levels.length >= 4}
                className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm disabled:opacity-30 hover:opacity-80 transition-all"
                style={{ border: '1px solid var(--theme-accent)' }}
              >
                <Plus size={14} /> Add TP Level
              </button>
              <span className="text-xs" style={{ color: 'var(--theme-text-muted)' }}>
                Total sell: {settings.tp_levels.reduce((s: number, l: TpLevel) => s + l.sell_percent, 0).toFixed(0)}%
              </span>
            </div>

            <h3 className="text-xl font-black mb-6 mt-8 glow-text uppercase tracking-wider flex items-center gap-3">
              <span className="w-2 h-2 bg-[var(--theme-accent)] rounded-full animate-pulse" style={{ boxShadow: '0 0 10px var(--theme-accent)' }}></span>
              Stop Loss Levels
            </h3>
            {settings.sl_levels.map((level: SlLevel, i: number) => {
              const slSum = settings.sl_levels.reduce((s: number, l: SlLevel) => s + l.sell_percent, 0);
              return (
                <div key={i} className="flex items-center gap-3 mb-3">
                  <span className="text-sm font-bold w-10" style={{ color: 'var(--theme-text-secondary)' }}>SL{i + 1}</span>
                  <div className="flex-1">
                    <label className="text-xs mb-1 block" style={{ color: 'var(--theme-text-muted)' }}>Trigger %</label>
                    <input
                      type="number"
                      value={level.trigger_percent}
                      onChange={(e) => {
                        const newLevels = [...settings.sl_levels];
                        newLevels[i] = { ...newLevels[i], trigger_percent: parseFloat(e.target.value) || 0 };
                        handleChange('sl_levels', newLevels);
                      }}
                      className="w-full px-3 py-2 rounded-xl transition-all"
                      step="any"
                    />
                  </div>
                  <div className="flex-1">
                    <label className="text-xs mb-1 block" style={{ color: 'var(--theme-text-muted)' }}>Sell %</label>
                    <input
                      type="number"
                      value={level.sell_percent}
                      onChange={(e) => {
                        const newLevels = [...settings.sl_levels];
                        newLevels[i] = { ...newLevels[i], sell_percent: parseFloat(e.target.value) || 0 };
                        handleChange('sl_levels', newLevels);
                      }}
                      className="w-full px-3 py-2 rounded-xl transition-all"
                      step="any"
                      min="0"
                      max="100"
                    />
                  </div>
                  <button
                    onClick={() => {
                      if (settings.sl_levels.length > 1) {
                        const newLevels = settings.sl_levels.filter((_: SlLevel, j: number) => j !== i);
                        handleChange('sl_levels', newLevels);
                      }
                    }}
                    disabled={settings.sl_levels.length <= 1}
                    className="mt-5 p-2 rounded-lg disabled:opacity-30 hover:opacity-80 transition-all"
                    title="Remove level"
                  >
                    <Trash2 size={16} />
                  </button>
                  {i === settings.sl_levels.length - 1 && slSum > 100 && (
                    <span className="text-xs mt-5" style={{ color: 'var(--theme-error, #ef4444)' }}>Sum: {slSum.toFixed(0)}% (&gt;100%)</span>
                  )}
                </div>
              );
            })}
            <div className="flex items-center gap-3 mt-2">
              <button
                onClick={() => {
                  if (settings.sl_levels.length < 4) {
                    handleChange('sl_levels', [...settings.sl_levels, { trigger_percent: -30, sell_percent: 25 }]);
                  }
                }}
                disabled={settings.sl_levels.length >= 4}
                className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm disabled:opacity-30 hover:opacity-80 transition-all"
                style={{ border: '1px solid var(--theme-accent)' }}
              >
                <Plus size={14} /> Add SL Level
              </button>
              <span className="text-xs" style={{ color: 'var(--theme-text-muted)' }}>
                Total sell: {settings.sl_levels.reduce((s: number, l: SlLevel) => s + l.sell_percent, 0).toFixed(0)}%
              </span>
            </div>
          </div>
        )}
        </div>
        );
      })}

      {/* Save Button */}
      <div className="flex justify-end gap-3 sticky bottom-0 py-4 px-6 rounded-xl" style={{
        backgroundColor: 'var(--theme-bg-card)',
        borderTop: '2px solid var(--theme-accent)',
        boxShadow: '0 -5px 20px var(--glow-color)'
      }}>
        <button
          onClick={handleSave}
          disabled={saving || !isBotStopped}
          className="flex items-center gap-2 px-6 py-3 rounded-xl disabled:opacity-50 transition-all hover:scale-105"
          title={!isBotStopped ? 'Stop the bot before saving settings' : ''}
        >
          <Save size={18} />
          {saving ? 'SAVING...' : isBotStopped ? 'SAVE SETTINGS' : 'STOP BOT TO SAVE'}
        </button>
      </div>
    </div>
  )
}
