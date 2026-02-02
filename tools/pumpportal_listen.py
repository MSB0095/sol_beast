#!/usr/bin/env python3
import asyncio
import json
import websockets
import datetime

URI = "wss://pumpportal.fun/api/data"

async def listen(duration=15):
    try:
        async with websockets.connect(URI) as ws:
            # subscribe to new token events
            await ws.send(json.dumps({"method": "subscribeNewToken"}))
            print(f"Subscribed to {URI} for {duration}s at {datetime.datetime.utcnow().isoformat()}Z")

            async def reader():
                async for msg in ws:
                    now = datetime.datetime.utcnow().isoformat() + "Z"
                    try:
                        obj = json.loads(msg)
                        print(now, json.dumps(obj, indent=2, ensure_ascii=False))
                    except Exception:
                        print(now, msg)

            task = asyncio.create_task(reader())
            await asyncio.sleep(duration)
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
    except Exception as e:
        print("Connection error:", e)

if __name__ == '__main__':
    asyncio.run(listen(15))
