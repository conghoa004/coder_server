
import asyncio
import os
import websockets

HOST = "0.0.0.0"
PORT = 8766

MYSQL_HOST = "127.0.0.1"
MYSQL_PORT = 3306

# Set the same secret on both machines
SECRET = "12345"

async def handler(websocket):
    writer = None
    tasks = []

    try:
        # Authenticate before opening the TCP connection
        token = await asyncio.wait_for(websocket.recv(), timeout=10)

        if not isinstance(token, str) or token != SECRET:
            await websocket.close(1008, "Unauthorized")
            return

        reader, writer = await asyncio.open_connection(
            MYSQL_HOST, MYSQL_PORT
        )

        print(f"[+] MySQL tunnel connected: {websocket.remote_address}")

        async def ws_to_tcp():
            async for data in websocket:
                if isinstance(data, bytes):
                    writer.write(data)
                    await writer.drain()

        async def tcp_to_ws():
            while True:
                data = await reader.read(65536)

                if not data:
                    break

                await websocket.send(data)

        tasks = [
            asyncio.create_task(ws_to_tcp()),
            asyncio.create_task(tcp_to_ws())
        ]

        done, pending = await asyncio.wait(
            tasks,
            return_when=asyncio.FIRST_COMPLETED
        )

        for task in pending:
            task.cancel()

        await asyncio.gather(*pending, return_exceptions=True)

        for task in done:
            if not task.cancelled():
                error = task.exception()
                if error:
                    raise error

    except Exception as e:
        print(f"[ERROR] {e}")

    finally:
        for task in tasks:
            if not task.done():
                task.cancel()

        if writer:
            writer.close()
            await writer.wait_closed()

        print("[-] Connection closed")


async def main():
    print(f"WebSocket Server: {HOST}:{PORT}")
    print(f"MySQL Target: {MYSQL_HOST}:{MYSQL_PORT}")

    async with websockets.serve(
        handler,
        HOST,
        PORT,
        max_size=None,
        max_queue=16,
        ping_interval=20,
        ping_timeout=20
    ):
        print("[READY] Waiting for connections...")
        await asyncio.Future()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("Server stopped")