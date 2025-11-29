import { useSettingsStore } from '../store/settingsStore'
import type { Settings as AppSettings } from '../store/settingsStore'
import { useState } from 'react'
import { Save, AlertCircle, CheckCircle, Server, Settings, Shield, Target, Zap, Database } from 'lucide-react'

export default function ConfigurationPanel() {
  const { settings, saving, error, saveSettings, updateSetting } = useSettingsStore()
  const [successMessage, setSuccessMessage] = useState('')

  if (!settings) return <div>Loading settings...</div>

  const handleSave = async () => {
    await saveSettings(settings)
    setSuccessMessage('Settings saved successfully!')
    setTimeout(() => setSuccessMessage(''), 3000)
  }

  const handleChange = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    updateSetting(key, value)
  }

  type SettingDef = {
    key: keyof AppSettings
    label: string
    type: 'checkbox' | 'number' | 'textarea' | 'select'
    help?: string
    options?: string[]
  }

  type Section = {
    title: string
    icon: JSX.Element
    settings: SettingDef[]
  }

  const sections: Section[] = [
    {
      title: 'RPC & WebSocket Configuration',
      icon: <Server className="w-5 h-5" />,
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
      icon: <Target className="w-5 h-5" />,
      settings: [
        { key: 'tp_percent' as const, label: 'Take Profit %', type: 'number', help: 'Sell when profit reaches this %' },
        { key: 'sl_percent' as const, label: 'Stop Loss %', type: 'number', help: 'Sell when loss reaches this %' },
        { key: 'timeout_secs' as const, label: 'Timeout (seconds)', type: 'number', help: 'Auto-sell after this duration' },
        { key: 'buy_amount' as const, label: 'Buy Amount (SOL)', type: 'number', help: 'SOL spent per buy' },
      ]
    },
    {
      title: 'Safety & Sniping Filters',
      icon: <Shield className="w-5 h-5" />,
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
      icon: <Database className="w-5 h-5" />,
      settings: [
        { key: 'max_holded_coins' as const, label: 'Max Holdings', type: 'number' },
        { key: 'max_create_to_buy_secs' as const, label: 'Max Create to Buy (secs)', type: 'number' },
      ]
    },
    {
      title: 'Helius Sender Configuration',
      icon: <Zap className="w-5 h-5" />,
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
      icon: <Settings className="w-5 h-5" />,
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
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3 mb-6">
        <div className="p-3 bg-primary/10 rounded-lg">
          <Settings className="w-6 h-6 text-primary" />
        </div>
        <div>
          <h2 className="text-2xl font-bold text-base-content uppercase tracking-wider">
            Configuration Panel
          </h2>
          <p className="text-base-content/60">Configure your SOL BEAST trading bot settings</p>
        </div>
      </div>

      {/* Error and Success Alerts */}
      {error && (
        <div role="alert" className="alert alert-error animate-fade-in-up">
          <AlertCircle className="w-5 h-5" />
          <div>
            <h3 className="font-bold uppercase tracking-wider">SYSTEM ERROR</h3>
            <div className="text-xs">{error}</div>
          </div>
        </div>
      )}

      {successMessage && (
        <div role="alert" className="alert alert-success animate-fade-in-up">
          <CheckCircle className="w-5 h-5" />
          <div>
            <h3 className="font-bold uppercase tracking-wider">SUCCESS</h3>
            <div className="text-xs">{successMessage}</div>
          </div>
        </div>
      )}

      {/* Configuration Sections using flyonui Accordion */}
      <div className="space-y-4">
        {sections.map((section, idx) => (
          <div key={idx} className="collapse collapse-arrow bg-base-200/50 border border-base-300 rounded-lg">
            <input type="checkbox" className="peer" defaultChecked={idx < 2} />
            <div className="collapse-title text-lg font-bold uppercase tracking-wider flex items-center gap-3 min-h-16 peer-checked:bg-primary/10">
              <div className="p-2 bg-primary/10 rounded-lg">
                {section.icon}
              </div>
              {section.title}
            </div>
            <div className="collapse-content">
              <div className="pt-4">
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                  {section.settings.map((setting) => (
                    <div key={String(setting.key)} className="form-control">
                      <label className="label-text mb-2 block">
                        {setting.label}
                        {setting.help && (
                          <span className="label-text-alt text-base-content/60 ml-2">
                            ({setting.help})
                          </span>
                        )}
                      </label>

                      {setting.type === 'checkbox' && (
                        <div className="flex items-center gap-3">
                          <input
                            type="checkbox"
                            className="checkbox checkbox-primary"
                            checked={(settings as AppSettings)[setting.key] as boolean}
                            onChange={(e) => handleChange(setting.key as keyof AppSettings, e.target.checked as AppSettings[typeof setting.key])}
                          />
                          <span className="label-text">
                            {settings[setting.key] ? 'ENABLED' : 'DISABLED'}
                          </span>
                        </div>
                      )}

                      {setting.type === 'number' && (
                        <input
                          type="number"
                          className="input input-primary"
                          value={(settings as AppSettings)[setting.key] as number}
                          onChange={(e) => handleChange(setting.key as keyof AppSettings, parseFloat(e.target.value) || 0 as AppSettings[typeof setting.key])}
                          step="any"
                        />
                      )}

                      {setting.type === 'textarea' && (
                        <textarea
                          className="textarea textarea-primary min-h-20"
                          value={((settings as AppSettings)[setting.key] as string[]).join('\n')}
                          onChange={(e) => handleChange(setting.key as keyof AppSettings, e.target.value.split('\n').filter(Boolean) as AppSettings[typeof setting.key])}
                          placeholder="Enter values separated by newlines..."
                        />
                      )}

                      {setting.type === 'select' && (
                        <select
                          className="select select-primary uppercase"
                          value={(settings as AppSettings)[setting.key] as string}
                          onChange={(e) => handleChange(setting.key as keyof AppSettings, e.target.value as AppSettings[typeof setting.key])}
                        >
                          {setting.options?.map((opt: string) => (
                            <option key={opt} value={opt}>
                              {opt.toUpperCase()}
                            </option>
                          ))}
                        </select>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Save Button */}
      <div className="sticky bottom-0 pt-4 mt-6">
        <div className="card bg-base-200/80 backdrop-blur-sm border border-primary/20 rounded-xl p-4">
          <div className="flex justify-end gap-3">
            <button
              onClick={handleSave}
              disabled={saving}
              className="btn btn-primary gap-2 uppercase tracking-wider font-bold"
            >
              <Save className="w-5 h-5" />
              {saving ? 'SAVING...' : 'SAVE SETTINGS'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
