import { useSettingsStore } from '../store/settingsStore'
import { useState } from 'react'
import { Save, AlertCircle, CheckCircle, ChevronDown, ChevronUp } from 'lucide-react'

export default function ConfigurationPanel() {
  const { settings, saving, error, saveSettings, updateSetting } = useSettingsStore()
  const [successMessage, setSuccessMessage] = useState('')
  const [collapsedSections, setCollapsedSections] = useState<Record<number, boolean>>({})

  if (!settings) return <div>Loading settings...</div>

  const handleSave = async () => {
    await saveSettings(settings)
    setSuccessMessage('Settings saved successfully!')
    setTimeout(() => setSuccessMessage(''), 3000)
  }

  const handleChange = <K extends keyof typeof settings>(key: K, value: any) => {
    updateSetting(key, value)
  }

  const toggleSection = (idx: number) => {
    setCollapsedSections(prev => ({ ...prev, [idx]: !prev[idx] }))
  }

  const sections = [
    {
      title: 'RPC & WebSocket Configuration',
      description: 'Configure Solana network endpoints and connection settings',
      settings: [
        {
          key: 'solana_ws_urls' as const,
          label: 'Solana WebSocket URLs',
          type: 'textarea',
          help: 'Enter WebSocket URLs separated by newlines (e.g., wss://api.mainnet-beta.solana.com/)'
        },
        {
          key: 'solana_rpc_urls' as const,
          label: 'Solana RPC URLs',
          type: 'textarea',
          help: 'Enter RPC URLs separated by newlines (e.g., https://api.mainnet-beta.solana.com/)'
        },
        {
          key: 'rotate_rpc' as const,
          label: 'Enable RPC Rotation',
          type: 'checkbox',
          help: 'Automatically rotate between RPC endpoints for load balancing'
        },
        {
          key: 'rpc_rotate_interval_secs' as const,
          label: 'RPC Rotation Interval (seconds)',
          type: 'number',
          help: 'How often to rotate RPC endpoints (default: 60)'
        },
        {
          key: 'max_subs_per_wss' as const,
          label: 'Max Subscriptions per WebSocket',
          type: 'number',
          help: 'Maximum subscriptions per WebSocket connection (default: 4)'
        },
        {
          key: 'sub_ttl_secs' as const,
          label: 'Subscription TTL (seconds)',
          type: 'number',
          help: 'Time-to-live for WebSocket subscriptions (default: 900)'
        },
        {
          key: 'wss_subscribe_timeout_secs' as const,
          label: 'WebSocket Subscribe Timeout (seconds)',
          type: 'number',
          help: 'Timeout waiting for WebSocket subscription confirmation (default: 6)'
        },
      ]
    },
    {
      title: 'Program Addresses',
      description: 'Solana program IDs (rarely need to change)',
      settings: [
        {
          key: 'pump_fun_program' as const,
          label: 'Pump.fun Program ID',
          type: 'text',
          help: 'Program address for pump.fun (default: 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P)'
        },
        {
          key: 'metadata_program' as const,
          label: 'Metadata Program ID',
          type: 'text',
          help: 'Metaplex metadata program address'
        },
      ]
    },
    {
      title: 'Trading Strategy',
      description: 'Configure profit targets, stop losses, and position management',
      settings: [
        { key: 'tp_percent' as const, label: 'Take Profit %', type: 'number', help: 'Sell when profit reaches this % (e.g., 30 = +30%)' },
        { key: 'sl_percent' as const, label: 'Stop Loss %', type: 'number', help: 'Sell when loss reaches this % (e.g., -20 = -20%)' },
        { key: 'timeout_secs' as const, label: 'Position Timeout (seconds)', type: 'number', help: 'Auto-sell after holding for this duration (default: 3600 = 1 hour)' },
        { key: 'buy_amount' as const, label: 'Buy Amount (SOL)', type: 'number', help: 'Amount of SOL to spend per buy (default: 0.1)', step: '0.001' },
        { key: 'max_holded_coins' as const, label: 'Max Concurrent Holdings', type: 'number', help: 'Maximum number of coins to hold simultaneously (default: 100)' },
      ]
    },
    {
      title: 'Safety & Sniping Filters',
      description: 'Configure buy filters to avoid expensive or risky tokens',
      settings: [
        { key: 'enable_safer_sniping' as const, label: 'Enable Safer Sniping', type: 'checkbox', help: 'Enable additional safety filters (recommended for beginners)' },
        { key: 'min_tokens_threshold' as const, label: 'Min Tokens Threshold', type: 'number', help: 'Minimum tokens you must receive (prevents buying expensive/late coins)' },
        { key: 'max_sol_per_token' as const, label: 'Max SOL per Token', type: 'number', help: 'Maximum price per token in SOL (additional price ceiling)', step: '0.00001' },
        { key: 'slippage_bps' as const, label: 'Slippage Tolerance (bps)', type: 'number', help: '500 = 5%. Higher = more likely to succeed but worse price' },
        { key: 'min_liquidity_sol' as const, label: 'Min Bonding Curve Liquidity (SOL)', type: 'number', help: 'Minimum SOL in bonding curve (too low = too early/risky)', step: '0.01' },
        { key: 'max_liquidity_sol' as const, label: 'Max Bonding Curve Liquidity (SOL)', type: 'number', help: 'Maximum SOL in bonding curve (too high = too late)', step: '0.01' },
        { key: 'max_create_to_buy_secs' as const, label: 'Max Create-to-Buy Time (seconds)', type: 'number', help: 'Maximum time from coin creation to buy attempt (lower = fresher coins)' },
      ]
    },
    {
      title: 'Helius Sender Configuration',
      description: 'Ultra-low latency transaction submission (optional, requires tips)',
      settings: [
        { key: 'helius_sender_enabled' as const, label: 'Enable Helius Sender', type: 'checkbox', help: 'Use Helius Sender for faster transaction submission' },
        {
          key: 'helius_sender_endpoint' as const,
          label: 'Helius Sender Endpoint',
          type: 'text',
          help: 'Global HTTPS or regional HTTP endpoint (e.g., http://slc-sender.helius-rpc.com/fast)'
        },
        {
          key: 'helius_api_key' as const,
          label: 'Helius API Key (Optional)',
          type: 'text',
          help: 'Optional API key for custom TPS limits (leave empty for default 15 TPS)'
        },
        { key: 'helius_use_swqos_only' as const, label: 'Use SWQOS-Only Mode', type: 'checkbox', help: 'Cost-optimized routing (0.000005 SOL min vs 0.001 SOL for dual routing)' },
        { key: 'helius_use_dynamic_tips' as const, label: 'Use Dynamic Tips', type: 'checkbox', help: 'Automatically fetch competitive tip amounts from Jito API (dual routing only)' },
        { key: 'helius_min_tip_sol' as const, label: 'Minimum Tip (SOL)', type: 'number', help: 'Minimum tip per transaction (0.001 for dual, 0.000005 for SWQOS)', step: '0.000001' },
        { key: 'helius_priority_fee_multiplier' as const, label: 'Priority Fee Multiplier', type: 'number', help: 'Multiplier for recommended fees (1.2 = 20% above recommended)', step: '0.1' },
        { key: 'helius_confirm_timeout_secs' as const, label: 'Confirmation Timeout (seconds)', type: 'number', help: 'How long to wait for transaction confirmation (0 = no wait)' },
      ]
    },
    {
      title: 'Caching & Performance',
      description: 'Configure caching and performance optimization settings',
      settings: [
        { key: 'cache_capacity' as const, label: 'Cache Capacity', type: 'number', help: 'Maximum number of cached items (default: 1024)' },
        { key: 'price_cache_ttl_secs' as const, label: 'Price Cache TTL (seconds)', type: 'number', help: 'How long to cache price data (default: 30)' },
        { key: 'price_source' as const, label: 'Price Data Source', type: 'select', options: ['wss', 'rpc', 'hybrid'], help: 'Source for price updates (wss = WebSocket, rpc = HTTP polling, hybrid = both)' },
      ]
    },
    {
      title: 'Advanced Options',
      description: 'Advanced validation and logging settings',
      settings: [
        { key: 'bonding_curve_strict' as const, label: 'Strict Bonding Curve Validation', type: 'checkbox', help: 'Enable strict validation of bonding curve state' },
        { key: 'bonding_curve_log_debounce_secs' as const, label: 'Bonding Curve Log Debounce (seconds)', type: 'number', help: 'Debounce time for bonding curve log messages (default: 300)' },
      ]
    }
  ]

  return (
    <div className="space-y-6">
      {/* Status Messages */}
      {error && (
        <div className="bg-red-900/20 border border-red-500 rounded-lg p-4 flex gap-3">
          <AlertCircle size={20} className="text-red-400 flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-red-200 font-semibold">Error</p>
            <p className="text-red-300 text-sm">{error}</p>
          </div>
        </div>
      )}

      {successMessage && (
        <div className="bg-green-900/20 border border-green-500 rounded-lg p-4 flex gap-3">
          <CheckCircle size={20} className="text-green-400 flex-shrink-0 mt-0.5" />
          <p className="text-green-200">{successMessage}</p>
        </div>
      )}

      {/* Info Banner */}
      <div className="bg-blue-900/20 border border-blue-500 rounded-lg p-4">
        <p className="text-blue-200 text-sm">
          ℹ️ Changes are saved to <code className="text-blue-300 bg-blue-900/30 px-1.5 py-0.5 rounded">config.toml</code> and take effect immediately. 
          Some settings may require restarting the bot to fully apply.
        </p>
      </div>

      {/* Settings Sections */}
      {sections.map((section, idx) => (
        <div key={idx} className="bg-sol-dark rounded-lg border border-gray-700 overflow-hidden">
          {/* Section Header (Collapsible) */}
          <button
            onClick={() => toggleSection(idx)}
            className="w-full px-6 py-4 flex items-center justify-between hover:bg-gray-800/50 transition-colors"
          >
            <div className="text-left">
              <h3 className="text-lg font-semibold text-sol-purple">{section.title}</h3>
              <p className="text-sm text-gray-400 mt-1">{section.description}</p>
            </div>
            {collapsedSections[idx] ? (
              <ChevronDown size={20} className="text-gray-400" />
            ) : (
              <ChevronUp size={20} className="text-gray-400" />
            )}
          </button>

          {/* Section Content */}
          {!collapsedSections[idx] && (
            <div className="px-6 py-4 border-t border-gray-700">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                {section.settings.map((setting) => {
                  const settingValue = settings[setting.key]
                  const fullWidth = setting.type === 'textarea' || setting.type === 'text'
                  
                  return (
                    <div key={setting.key} className={fullWidth ? 'md:col-span-2' : ''}>
                      <label className="block text-sm font-medium text-gray-300 mb-2">
                        {setting.label}
                      </label>
                      
                      {(setting as any).help && (
                        <p className="text-xs text-gray-500 mb-2">{(setting as any).help}</p>
                      )}

                      {setting.type === 'checkbox' && (
                        <div className="flex items-center">
                          <input
                            type="checkbox"
                            checked={settingValue as any}
                            onChange={(e) => handleChange(setting.key, e.target.checked)}
                            className="w-5 h-5 rounded border-gray-600 bg-sol-darker text-sol-purple focus:ring-sol-purple focus:ring-offset-0 cursor-pointer"
                          />
                          <span className="ml-2 text-sm text-gray-400">
                            {settingValue ? 'Enabled' : 'Disabled'}
                          </span>
                        </div>
                      )}

                      {setting.type === 'number' && (
                        <input
                          type="number"
                          value={settingValue as any}
                          onChange={(e) => handleChange(setting.key, parseFloat(e.target.value) || 0)}
                          className="w-full px-3 py-2 bg-sol-darker border border-gray-600 rounded-lg text-white focus:border-sol-purple focus:outline-none focus:ring-1 focus:ring-sol-purple"
                          step={(setting as any).step || 'any'}
                        />
                      )}

                      {setting.type === 'text' && (
                        <input
                          type="text"
                          value={settingValue as any || ''}
                          onChange={(e) => handleChange(setting.key, e.target.value)}
                          className="w-full px-3 py-2 bg-sol-darker border border-gray-600 rounded-lg text-white focus:border-sol-purple focus:outline-none focus:ring-1 focus:ring-sol-purple font-mono text-sm"
                          placeholder={`Enter ${setting.label.toLowerCase()}`}
                        />
                      )}

                      {setting.type === 'textarea' && (
                        <textarea
                          value={Array.isArray(settingValue) ? (settingValue as string[]).join('\n') : ''}
                          onChange={(e) => handleChange(setting.key, e.target.value.split('\n').filter(Boolean))}
                          className="w-full px-3 py-2 bg-sol-darker border border-gray-600 rounded-lg text-white focus:border-sol-purple focus:outline-none focus:ring-1 focus:ring-sol-purple min-h-24 font-mono text-sm"
                          placeholder={`Enter ${setting.label.toLowerCase()} (one per line)`}
                        />
                      )}

                      {setting.type === 'select' && (
                        <select
                          value={settingValue as string}
                          onChange={(e) => handleChange(setting.key, e.target.value)}
                          className="w-full px-3 py-2 bg-sol-darker border border-gray-600 rounded-lg text-white focus:border-sol-purple focus:outline-none focus:ring-1 focus:ring-sol-purple"
                        >
                          {(setting as any).options?.map((opt: string) => (
                            <option key={opt} value={opt}>{opt}</option>
                          ))}
                        </select>
                      )}
                    </div>
                  )
                })}
              </div>
            </div>
          )}
        </div>
      ))}

      {/* Save Button - Sticky */}
      <div className="sticky bottom-0 bg-sol-dark/95 backdrop-blur-sm py-4 px-6 rounded-lg border border-gray-700 shadow-xl">
        <div className="flex items-center justify-between">
          <p className="text-sm text-gray-400">
            Make sure to save your changes before leaving this page
          </p>
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-2 px-6 py-2.5 bg-sol-purple text-black font-semibold rounded-lg hover:bg-opacity-90 disabled:opacity-50 disabled:cursor-not-allowed transition-all shadow-lg"
          >
            <Save size={18} />
            {saving ? 'Saving...' : 'Save All Settings'}
          </button>
        </div>
      </div>
    </div>
  )
}
