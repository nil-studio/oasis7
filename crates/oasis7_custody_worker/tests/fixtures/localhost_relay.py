#!/usr/bin/env python3
"""Test-only TCP relay: TLS bytes stay end-to-end Host→actual RP localhost.
Trusted root supplies one localhost high port. No HTTP parsing, signing or secrets.
"""
import selectors, socket, socketserver, sys
class Relay(socketserver.BaseRequestHandler):
    def handle(self):
        with socket.create_connection(('host.docker.internal', self.server.server_address[1]), timeout=5) as upstream:
            upstream.settimeout(None)
            with selectors.DefaultSelector() as events:
                events.register(self.request, selectors.EVENT_READ, upstream)
                events.register(upstream, selectors.EVENT_READ, self.request)
                while True:
                    ready = events.select(timeout=15)
                    if not ready:
                        return
                    for key, _ in ready:
                        chunk = key.fileobj.recv(65536)
                        if not chunk:
                            return
                        key.data.sendall(chunk)
class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = False
    daemon_threads = True
if __name__ == '__main__':
    if len(sys.argv) != 2 or not sys.argv[1].isdecimal():
        raise SystemExit(2)
    port = int(sys.argv[1])
    if not 1024 <= port <= 65535:
        raise SystemExit(2)
    with Server(('127.0.0.1', port), Relay) as server:
        server.serve_forever(poll_interval=0.1)
