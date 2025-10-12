import struct
import asyncio
from aiohttp import web
from prometheus_client import Gauge, generate_latest

PORT = 7004

temperature_gauge = Gauge('temperature', 'Current temperature in Celsius')
humidity_gauge = Gauge('humidity', 'Current humidity percentage')
co2_gauge = Gauge('co2_ppm', 'Current CO2 concentration in ppm')

async def handle_metrics(request):
    data = generate_latest()
    return web.Response(body=data, content_type='text/plain')

async def start_udp_server():
    loop = asyncio.get_event_loop()

    class UDPProtocol:
        def connection_made(self, transport):
            pass

        def datagram_received(self, data, addr):
            # print(f'received {len(data)} bytes from {addr}')
            try:
                if len(data) == 10:
                    temperature, humidity, co2 = struct.unpack('>ffH', data)
                    print(f"{addr}: temperature {temperature:.2f} °C, humidity: {humidity:.2f} %, co2_ppm: {co2} ppm")
                    temperature_gauge.set(temperature)
                    humidity_gauge.set(humidity)
                    co2_gauge.set(co2)
                else:
                    print(f"invalid data len: {len(data)}")
            except Exception as e:
                print(f"unpack data: {e}")

        def error_received(self, exc):
            print(f"udp err: {exc}")

        def connection_lost(self, exc):
            print(f"connection closed: {exc}")

    transport, protocol = await loop.create_datagram_endpoint(
        UDPProtocol,
        local_addr=('0.0.0.0', PORT)
    )
    print(f"listen udp {PORT}")
    return transport

async def main():
    app = web.Application()
    app.router.add_get('/metrics', handle_metrics)
    runner = web.AppRunner(app)
    await runner.setup()
    # site = web.TCPSite(runner, '127.0.0.1', PORT)
    site = web.TCPSite(runner, '0.0.0.0', PORT)
    await site.start()

    print(f"listen tcp {PORT}")

    udp_transport = await start_udp_server()
    try:
        while True:
            await asyncio.sleep(1)
    except KeyboardInterrupt:
        pass
    finally:
        udp_transport.close()
        await runner.cleanup()

if __name__ == "__main__":
    asyncio.run(main())
