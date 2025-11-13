import { useSettingsStore } from '../store/settingsStore'
import { useState } from 'react'
import { Save, AlertCircle, CheckCircle } from 'lucide-react'

export default function ConfigurationPanel() {
  const { settings, saving, error, saveSettings, updateSetting } = useSettingsStore()
  const [successMessage, setSuccessMessage] = useState('')

  if (!settings) return <div>Loading settings...</div>

  const handleSave = async () => {
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
      {error && (
        <div className="bg-red-900/20 border border-red-500/50 rounded-xl p-4 flex gap-3 backdrop-blur-sm shadow-card">
          <AlertCircle size={20} className="text-red-400 flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-red-200 font-semibold">Error</p>
            <p className="text-red-300 text-sm">{error}</p>
          </div>
        </div>
      )}

      {successMessage && (
        <div className="bg-green-900/20 border border-green-500/50 rounded-xl p-4 flex gap-3 backdrop-blur-sm shadow-card">
          <CheckCircle size={20} className="text-green-400 flex-shrink-0 mt-0.5" />
          <p className="text-green-200">{successMessage}</p>
        </div>
      )}

      {sections.map((section, idx) => (
        <div key={idx} className="card-enhanced rounded-xl p-6">
          <h3 className="text-lg font-semibold mb-6 gradient-text">{section.title}</h3>
          
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {section.settings.map((setting) => (
              <div key={setting.key}>
                <label className="block text-sm font-medium text-gray-300 mb-2">
                  {setting.label}
                  {(setting as any).help && <span className="text-gray-500 text-xs ml-2">({(setting as any).help})</span>}
                </label>

                {setting.type === 'checkbox' && (
                  <input
                    type="checkbox"
                    checked={settings[setting.key] as any}
                    onChange={(e) => handleChange(setting.key, e.target.checked)}
                    className="w-5 h-5 rounded border-gray-600 bg-sol-dark cursor-pointer"
                  />
                )}

                {setting.type === 'number' && (
                  <input
                    type="number"
                    value={settings[setting.key] as any}
                    onChange={(e) => handleChange(setting.key, parseFloat(e.target.value))}
                    className="w-full px-3 py-2 bg-sol-darker border border-gray-600 rounded-xl text-white focus:border-sol-purple focus:outline-none focus:shadow-glow transition-all"
                    step="any"
                  />
                )}

                {setting.type === 'textarea' && (
                  <textarea
                    value={(settings[setting.key] as string[]).join('\n')}
                    onChange={(e) => handleChange(setting.key, e.target.value.split('\n').filter(Boolean))}
                    className="w-full px-3 py-2 bg-sol-darker border border-gray-600 rounded-xl text-white focus:border-sol-purple focus:outline-none focus:shadow-glow min-h-20 font-mono text-sm transition-all"
                  />
                )}

                {setting.type === 'select' && (
                  <select
                    value={settings[setting.key] as string}
                    onChange={(e) => handleChange(setting.key, e.target.value)}
                    className="w-full px-3 py-2 bg-sol-darker border border-gray-600 rounded-xl text-white focus:border-sol-purple focus:outline-none focus:shadow-glow transition-all"
                  >
                    {(setting as any).options?.map((opt: string) => (
                      <option key={opt} value={opt}>{opt}</option>
                    ))}
                  </select>
                )}
              </div>
            ))}
          </div>
        </div>
      ))}

      {/* Save Button */}
      <div className="flex justify-end gap-3 sticky bottom-0 card-enhanced py-4 px-6 rounded-xl">
        <button
          onClick={handleSave}
          disabled={saving}
          className="flex items-center gap-2 px-6 py-3 bg-gradient-to-r from-sol-purple to-sol-cyan text-black font-semibold rounded-xl hover:shadow-glow disabled:opacity-50 transition-all hover:scale-105 shadow-card"
        >
          <Save size={18} />
          {saving ? 'Saving...' : 'Save Settings'}
        </button>
      </div>
    </div>
  )
}
