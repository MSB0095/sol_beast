// API client utilities for frontend
import axios, { AxiosInstance } from 'axios'
import { RUNTIME_MODE } from '../config'
import { Settings } from '../store/settingsStore'

const API_BASE = '/api'

class ApiClient {
  private client: AxiosInstance
  private apiDisabled: boolean

  constructor() {
    this.client = axios.create({
      baseURL: API_BASE,
      timeout: 10000,
      headers: {
        'Content-Type': 'application/json',
      },
    })
    this.apiDisabled = RUNTIME_MODE === 'frontend-wasm'
  }

  // Health check
  async checkHealth() {
    if (this.apiDisabled) {
      return Promise.resolve({ data: { status: 'no-api-runtime' } })
    }
    return this.client.get('/health')
  }

  // Get statistics
  async getStats() {
    if (this.apiDisabled) {
      return Promise.resolve({ data: { total_buys: 0, total_sells: 0, total_profit: 0, current_holdings: [], uptime_secs: 0, last_activity: new Date().toISOString(), running_state: 'stopped', mode: 'dry-run' } })
    }
    return this.client.get('/stats')
  }

  // Get current settings
  async getSettings(): Promise<Settings> {
    if (this.apiDisabled) {
      return Promise.resolve({} as Settings)
    }
    const response = await this.client.get('/settings')
    return response.data
  }

  // Update settings
  async updateSettings(settings: Partial<Settings>) {
    if (this.apiDisabled) {
      return Promise.resolve({ data: { status: 'ok' } })
    }
    return this.client.post('/settings', settings)
  }

  // Helper for batch updates
  async batchUpdateSettings(updates: Record<string, any>) {
    return this.updateSettings(updates as any)
  }

  // Error handler
  handleError(error: unknown): string {
    if (axios.isAxiosError(error)) {
      return error.response?.data?.message || error.message
    }
    return 'An unknown error occurred'
  }
}

export const apiClient = new ApiClient()
