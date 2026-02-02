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
        if (data.type === 'detected-coin') {
          console.log('New coin detected:', data.coin);
          // Refresh stats to get updated detected coins list
          fetchStats();
        } else if (data.type === 'price-update') {
          console.log('Price update:', data);
          // Refresh stats to get updated holdings with new prices
          fetchStats();
        } else if (data.type === 'holding-update') {
          console.log('Holdings updated:', data);
          // Refresh stats
          fetchStats();
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
  }, [fetchStats]);
};

export default useWebSocket;