// WebSocket management is handled entirely by botStore.initializeConnection.
// This hook is kept as a no-op for compatibility with App.tsx imports.
const useWebSocket = () => {
  // No-op: botStore already manages the single WS connection and
  // dispatches price-update, detected-coin, and holding-update messages.
};

export default useWebSocket;