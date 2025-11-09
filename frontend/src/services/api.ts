// API client utilities for frontend
import axios, { AxiosInstance } from 'axios'
import { Settings } from '../store/settingsStore'

const API_BASE = '/api'

class ApiClient {
  private client: AxiosInstance

  constructor() {
    this.client = axios.create({
      baseURL: API_BASE,
      timeout: 10000,
      headers: {
        'Content-Type': 'application/json',
      },
    })
  }

  // Health check
  async checkHealth() {
    return this.client.get('/health')
  }

  // Get statistics
  async getStats() {
    return this.client.get('/stats')
  }

  // Get current settings
  async getSettings(): Promise<Settings> {
    const response = await this.client.get('/settings')
    return response.data
  }

  // Update settings
  async updateSettings(settings: Partial<Settings>) {
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
