import React from 'react';

interface State {
  error: Error | null;
}

/**
 * Catches any uncaught render error and shows it inline instead of leaving
 * the whole app blank. Without this, a throw in any descendant unmounts
 * the entire tree (React 18 default), which is the failure mode that
 * previously made the FixDrawer / validation panel feel like a "blank page
 * with no way back".
 */
export class ErrorBoundary extends React.Component<{ children: React.ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    // eslint-disable-next-line no-console
    console.error('[ErrorBoundary] caught', error, info.componentStack);
  }

  reset = (): void => this.setState({ error: null });

  render() {
    if (this.state.error) {
      return (
        <div className="flex h-screen w-screen flex-col items-center justify-center gap-3 bg-gray-50 p-6 text-sm text-gray-700">
          <div className="max-w-xl rounded-md border border-red-200 bg-red-50 p-4 text-red-800">
            <div className="font-semibold">UI crashed</div>
            <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-words text-xs">
              {this.state.error.message}
              {'\n\n'}
              {this.state.error.stack}
            </pre>
          </div>
          <button
            onClick={this.reset}
            className="rounded-md bg-blue-500 px-4 py-2 text-white hover:bg-blue-600"
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
