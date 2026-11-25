
import asyncio
import websockets

SERVER = "wss://specification-amd-residence-lead.trycloudflare.com"

LOCAL_HOST = "127.0.0.1"
LOCAL_PORT = 13306

SECRET = "12345"


async def handle_client(reader, writer):
    address = writer.get_extra_info("peername")
    tasks = []

    try:
        async with websockets.connect(
            SERVER,
            max_size=None,
            ping_interval=20,
            ping_timeout=20
        ) as websocket:

            # Authenticate
            await websocket.send(SECRET)

            print(f"[+] Connected: {address}")

            async def tcp_to_ws():
                while True:
                    data = await reader.read(65536)

                    if not data:
                        break

                    await websocket.send(data)

            async def ws_to_tcp():
                async for data in websocket:
                    if isinstance(data, bytes):
                        writer.write(data)
                        await writer.drain()

            tasks = [
                asyncio.create_task(tcp_to_ws()),
                asyncio.create_task(ws_to_tcp())
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
        print(f"[ERROR] {address}: {e}")

    finally:
        for task in tasks:
            if not task.done():
                task.cancel()

        writer.close()

        try:
            await writer.wait_closed()
        except Exception:
            pass

        print(f"[-] Disconnected: {address}")


async def main():
    server = await asyncio.start_server(
        handle_client,
        LOCAL_HOST,
        LOCAL_PORT
    )

    print("=" * 45)
    print("       TCP TO WEBSOCKET CLIENT")
    print("=" * 45)
    print(f"Local TCP: {LOCAL_HOST}:{LOCAL_PORT}")
    print(f"Remote WS: {SERVER}")
    print("=" * 45)

    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("Client stopped")
