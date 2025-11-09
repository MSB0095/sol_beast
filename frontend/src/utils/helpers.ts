// Utils for formatting and validation

export const formatSolAmount = (amount: number, decimals = 4): string => {
  return amount.toFixed(decimals)
}

export const formatPercent = (percent: number, decimals = 2): string => {
  const sign = percent >= 0 ? '+' : ''
  return `${sign}${percent.toFixed(decimals)}%`
}

export const formatUSD = (amount: number): string => {
  return `$${amount.toFixed(2)}`
}

export const formatTimestamp = (timestamp: string): string => {
  return new Date(timestamp).toLocaleTimeString()
}

export const truncateAddress = (address: string, chars = 4): string => {
  return `${address.slice(0, chars)}...${address.slice(-chars)}`
}

export const isValidPublicKey = (key: string): boolean => {
  try {
    // Basic validation - valid base58 and reasonable length
    return key.length >= 43 && key.length <= 44
  } catch {
    return false
  }
}

export const isValidURL = (url: string): boolean => {
  try {
    new URL(url)
    return true
  } catch {
    return false
  }
}

export const isValidSOLAmount = (amount: number): boolean => {
  return amount > 0 && amount <= 1000
}

export const calculateROI = (entryPrice: number, currentPrice: number): number => {
  return ((currentPrice - entryPrice) / entryPrice) * 100
}

export const calculateProfitLoss = (buyAmount: number, currentPrice: number): number => {
  return currentPrice - buyAmount
}

// Number formatting
export const abbreviateNumber = (num: number): string => {
  if (num >= 1_000_000) {
    return (num / 1_000_000).toFixed(1) + 'M'
  }
  if (num >= 1_000) {
    return (num / 1_000).toFixed(1) + 'K'
  }
  return num.toString()
}

// Debounce helper
export const debounce = <T extends (...args: any[]) => any>(
  fn: T,
  delay: number
): ((...args: Parameters<T>) => void) => {
  let timeoutId: ReturnType<typeof setTimeout> | undefined

  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId)
    timeoutId = setTimeout(() => fn(...args), delay)
  }
}

// Local storage helpers
export const storage = {
  get: (key: string): any => {
    try {
      const item = localStorage.getItem(key)
      return item ? JSON.parse(item) : null
    } catch {
      return null
    }
  },
  set: (key: string, value: any): void => {
    localStorage.setItem(key, JSON.stringify(value))
  },
  remove: (key: string): void => {
    localStorage.removeItem(key)
  },
  clear: (): void => {
    localStorage.clear()
  },
}
