import { useEffect } from 'react';
import { useBotStore } from "../store/botStore";
import { WS_URL } from "../config";
import { FEATURES } from "../config";

const useWebSocket = () => {
  const { fetchStats } = useBotStore();

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

        // Handle different message types
        // NOTE: botStore.ts now handles state updates directly from WebSocket.
        // We log here for debugging but avoid calling fetchStats() to prevent
        // thrashing the backend and overwriting real-time updates.

        /*
        if (data.type === 'detected-coin') {
          console.log('New coin detected:', data.coin);
          // Refresh stats to get updated detected coins list
          fetchStats(); // Handled by botStore
        } else if (data.type === 'price-update') {
          console.log('Price update:', data);
          // Refresh stats to get updated holdings with new prices
          fetchStats(); // Handled by botStore
        } else if (data.type === 'holding-update') {
          console.log('Holdings updated:', data);
          // Refresh stats
          fetchStats(); // Handled by botStore
        }
        */
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
  }, [fetchStats]);
};

export default useWebSocket;