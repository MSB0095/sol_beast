// Toast notification utility using react-hot-toast
// Centralized toast configuration and helper functions

import toast, { Toaster as HotToaster, Toast } from 'react-hot-toast'

// Export Toaster component for use in App.tsx
export const Toaster = HotToaster

// Custom toast styles matching the app's theme
const toastOptions = {
  // Default styles
  style: {
    borderRadius: '12px',
    background: '#1e293b', // Gray-800
    color: '#f1f5f9', // Gray-100
    padding: '16px',
    boxShadow: '0 10px 15px -3px rgba(0, 0, 0, 0.3), 0 4px 6px -2px rgba(0, 0, 0, 0.2)',
    maxWidth: '500px',
  },
  // Icon themes
  success: {
    iconTheme: {
      primary: '#10b981', // Green-500
      secondary: '#f1f5f9',
    },
  },
  error: {
    iconTheme: {
      primary: '#ef4444', // Red-500
      secondary: '#f1f5f9',
    },
  },
  loading: {
    iconTheme: {
      primary: '#8b5cf6', // Purple-500
      secondary: '#f1f5f9',
    },
  },
  // Duration
  duration: 4000,
}

// Success toast
export const successToast = (message: string, details?: string) => {
  return toast.success(
    <div className="flex flex-col gap-1">
      <div className="font-semibold">{message}</div>
      {details && <div className="text-sm text-gray-400">{details}</div>}
    </div>,
    toastOptions
  )
}

// Error toast
export const errorToast = (message: string, details?: string) => {
  return toast.error(
    <div className="flex flex-col gap-1">
      <div className="font-semibold">{message}</div>
      {details && <div className="text-sm text-gray-400">{details}</div>}
    </div>,
    {
      ...toastOptions,
      duration: 6000, // Longer for errors
    }
  )
}

// Info toast
export const infoToast = (message: string, details?: string) => {
  return toast(
    <div className="flex flex-col gap-1">
      <div className="font-semibold">{message}</div>
      {details && <div className="text-sm text-gray-400">{details}</div>}
    </div>,
    {
      ...toastOptions,
      icon: 'ℹ️',
    }
  )
}

// Loading toast (returns toast ID for dismissal)
export const loadingToast = (message: string) => {
  return toast.loading(message, toastOptions)
}

// Transaction toast helpers
export const transactionSubmittedToast = (signature: string) => {
  return successToast(
    'Transaction Submitted',
    `Signature: ${signature.slice(0, 8)}...${signature.slice(-8)}`
  )
}

export const transactionConfirmedToast = (signature: string, type: 'buy' | 'sell') => {
  const action = type === 'buy' ? 'Purchase' : 'Sale'
  return successToast(
    `${action} Confirmed!`,
    `Transaction: ${signature.slice(0, 8)}...${signature.slice(-8)}`
  )
}

// Transaction toast with Solscan link
export const transactionToastWithLink = (
  signature: string, 
  type: 'buy' | 'sell', 
  status: 'submitted' | 'confirmed'
) => {
  const action = type === 'buy' ? 'Purchase' : 'Sale'
  const statusText = status === 'submitted' ? 'Submitted' : 'Confirmed'
  
  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(signature)
      toast.success('Signature copied to clipboard!', { duration: 2000 })
    } catch (err) {
      console.error('Failed to copy signature:', err)
    }
  }
  
  return toast.custom(
    (t: Toast) => (
      <div
        className={`${
          t.visible ? 'animate-enter' : 'animate-leave'
        } max-w-md w-full bg-gray-800 shadow-lg rounded-xl pointer-events-auto flex flex-col gap-3 p-4 border-2 border-green-500/50`}
      >
        <div className="flex items-start gap-3">
          <div className="flex-shrink-0 text-2xl">
            {status === 'confirmed' ? '✅' : '📤'}
          </div>
          <div className="flex-1">
            <p className="text-sm font-semibold text-white">
              {action} {statusText}!
            </p>
            <button
              onClick={copyToClipboard}
              className="mt-1 text-xs text-gray-400 font-mono break-all hover:text-purple-400 transition-colors cursor-pointer text-left"
              title="Click to copy signature"
            >
              {signature.slice(0, 16)}...{signature.slice(-16)}
            </button>
          </div>
          <button
            onClick={() => toast.dismiss(t.id)}
            className="flex-shrink-0 text-gray-400 hover:text-white transition-colors"
          >
            ✕
          </button>
        </div>
        <a
          href={`https://solscan.io/tx/${signature}`}
          target="_blank"
          rel="noopener noreferrer"
          className="w-full py-2 px-3 bg-purple-600 hover:bg-purple-700 transition-colors rounded-lg text-sm font-semibold text-center flex items-center justify-center gap-2"
          onClick={() => toast.dismiss(t.id)}
        >
          View on Solscan
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
            <polyline points="15 3 21 3 21 9"></polyline>
            <line x1="10" y1="14" x2="21" y2="3"></line>
          </svg>
        </a>
      </div>
    ),
    {
      duration: status === 'confirmed' ? 8000 : 6000,
    }
  )
}

// Wallet connection toast
export const walletConnectRequiredToast = () => {
  return errorToast(
    'Wallet Not Connected',
    'Please connect your wallet first to perform this action.'
  )
}

// Update loading toast with success/error
export const updateLoadingToast = (
  toastId: string,
  success: boolean,
  message: string,
  details?: string
) => {
  if (success) {
    toast.success(
      <div className="flex flex-col gap-1">
        <div className="font-semibold">{message}</div>
        {details && <div className="text-sm text-gray-400">{details}</div>}
      </div>,
      { id: toastId, ...toastOptions }
    )
  } else {
    toast.error(
      <div className="flex flex-col gap-1">
        <div className="font-semibold">{message}</div>
        {details && <div className="text-sm text-gray-400">{details}</div>}
      </div>,
      { id: toastId, ...toastOptions, duration: 6000 }
    )
  }
}

// Dismiss specific toast
export const dismissToast = (toastId: string) => {
  toast.dismiss(toastId)
}

// Dismiss all toasts
export const dismissAllToasts = () => {
  toast.dismiss()
}

// Export the raw toast for custom uses
export { toast }
