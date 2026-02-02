import { useEffect } from 'react';
import { useSettingsStore } from "../store/settingsStore";
import { WS_URL } from "../config";
import { FEATURES } from "../config";

const useWebSocket = () => {
  const { updateDetectedCoins } = useSettingsStore();

  useEffect(() => {
    if (!WS_URL || !FEATURES.WEBSOCKET) return;

    const ws = new WebSocket(WS_URL);

    ws.onopen = () => {
      console.log('WebSocket connection established');
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        console.log('Received WebSocket message:', data);

        // Update detected coins in the store
        if (data && data.type === 'new-coin') {
          updateDetectedCoins(data.coin);
        }
      } catch (error) {
        console.error('Error parsing WebSocket message:', error);
      }
    };

    ws.onerror = (error) => {
      console.error('WebSocket error:', error);
    };

    ws.onclose = () => {
      console.log('WebSocket connection closed');
    };

    return () => {
      ws.close();
    };
  }, [updateDetectedCoins]);
};

export default useWebSocket;