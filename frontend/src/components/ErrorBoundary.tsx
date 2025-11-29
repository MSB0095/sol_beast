import React, { ReactNode } from 'react'
import { AlertCircle, RefreshCw, Bug, Terminal } from 'lucide-react'

interface ErrorBoundaryProps {
  children: ReactNode
  fallback?: ReactNode
}

interface ErrorBoundaryState {
  hasError: boolean
  error: Error | null
  errorInfo: React.ErrorInfo | null
}

export class ErrorBoundary extends React.Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  constructor(props: ErrorBoundaryProps) {
    super(props)
    this.state = { hasError: false, error: null, errorInfo: null }
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error, errorInfo: null }
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('Error caught by boundary:', error, errorInfo)
    this.setState({ errorInfo })
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: null, errorInfo: null })
  }

  handleReload = () => {
    window.location.reload()
  }

  render() {
    if (this.state.hasError) {
      return (
        this.props.fallback || (
          <div className="min-h-screen flex items-center justify-center p-4 bg-base-200">
            <div className="card bg-base-100 shadow-lg border border-error/20 max-w-2xl w-full">
              <div className="card-body">
                <div className="alert alert-error mb-4">
                  <AlertCircle className="w-6 h-6" />
                  <div>
                    <h3 className="font-bold text-lg">Application Error</h3>
                    <div className="text-xs">
                      An unexpected error occurred in the Sol Beast application.
                    </div>
                  </div>
                </div>

                <div className="space-y-4">
                  {/* Error Message */}
                  <div className="bg-base-200 rounded-lg p-4">
                    <div className="flex items-center gap-2 mb-2">
                      <Bug className="w-4 h-4 text-error" />
                      <h4 className="font-semibold text-error">Error Details</h4>
                    </div>
                    <p className="text-sm text-base-content/80 font-mono break-all">
                      {this.state.error?.message || 'Unknown error occurred'}
                    </p>
                  </div>

                  {/* Stack Trace (Development Mode) */}
                  {process.env.NODE_ENV === 'development' && this.state.errorInfo && (
                    <div className="bg-base-200 rounded-lg p-4">
                      <div className="flex items-center gap-2 mb-2">
                        <Terminal className="w-4 h-4 text-warning" />
                        <h4 className="font-semibold text-warning">Stack Trace</h4>
                      </div>
                      <details className="cursor-pointer">
                        <summary className="text-sm text-base-content/60 mb-2">
                          Show technical details
                        </summary>
                        <pre className="text-xs text-base-content/60 overflow-x-auto whitespace-pre-wrap font-mono bg-base-300 p-3 rounded">
                          {this.state.errorInfo.componentStack}
                        </pre>
                      </details>
                    </div>
                  )}

                  {/* Actions */}
                  <div className="flex gap-3 justify-center pt-4">
                    <button
                      onClick={this.handleRetry}
                      className="btn btn-primary btn-sm gap-2"
                    >
                      <RefreshCw className="w-4 h-4" />
                      Try Again
                    </button>
                    <button
                      onClick={this.handleReload}
                      className="btn btn-secondary btn-sm gap-2"
                    >
                      <RefreshCw className="w-4 h-4" />
                      Reload Page
                    </button>
                  </div>
                </div>

                {/* Help Text */}
                <div className="mt-6 text-center">
                  <p className="text-xs text-base-content/60">
                    If this problem persists, try refreshing the page or contact support.
                  </p>
                </div>
              </div>
            </div>
          </div>
        )
      )
    }

    return this.props.children
  }
}
