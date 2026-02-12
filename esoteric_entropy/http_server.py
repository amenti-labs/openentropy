"""Lightweight HTTP entropy server — ANU QRNG API compatible.

Uses only Python stdlib (http.server + json). No Flask/FastAPI required.
Compatible with quantum-llama.cpp QRNG backend.
"""

from __future__ import annotations

import json
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlparse

from esoteric_entropy import __version__


def run_server(pool, host: str = "127.0.0.1", port: int = 8042) -> None:
    """Start the entropy HTTP server."""

    class EntropyHandler(BaseHTTPRequestHandler):
        server_version = f"esoteric-entropy/{__version__}"

        def do_GET(self) -> None:  # noqa: N802
            parsed = urlparse(self.path)
            path = parsed.path.rstrip("/")
            params = parse_qs(parsed.query)

            routes = {
                "/api/v1/random": self._handle_random,
                "/health": self._handle_health,
                "/sources": self._handle_sources,
                "/pool/status": self._handle_pool_status,
            }

            handler = routes.get(path)
            if handler:
                handler(params)
            else:
                self._json_response(404, {"error": "not found", "endpoints": list(routes.keys())})

        def _handle_random(self, params: dict) -> None:
            length = int(params.get("length", [1024])[0])
            dtype = params.get("type", ["hex16"])[0]
            length = max(1, min(length, 65536))

            data = pool.get_random_bytes(length)

            if dtype == "hex16":
                result = [data[i:i+2].hex() for i in range(0, len(data) - 1, 2)]
            elif dtype == "uint8":
                result = list(data)
            elif dtype == "uint16":
                import struct
                n = len(data) // 2
                result = list(struct.unpack(f"<{n}H", data[:n*2]))
            else:
                self._json_response(400, {"error": f"unknown type: {dtype}"})
                return

            self._json_response(200, {
                "type": dtype,
                "length": len(result),
                "data": result,
                "success": True,
            })

        def _handle_health(self, params: dict) -> None:
            report = pool.health_report()
            self._json_response(200, {
                "status": "healthy" if report["healthy"] > 0 else "degraded",
                "sources_healthy": report["healthy"],
                "sources_total": report["total"],
                "version": __version__,
                "uptime": time.monotonic(),
            })

        def _handle_sources(self, params: dict) -> None:
            self._json_response(200, {
                "sources": [
                    {"name": s.source.name, "description": s.source.description, "healthy": s.healthy}
                    for s in pool.sources
                ]
            })

        def _handle_pool_status(self, params: dict) -> None:
            self._json_response(200, pool.health_report())

        def _json_response(self, code: int, data: dict) -> None:
            body = json.dumps(data).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, fmt, *args) -> None:
            # Quieter logging
            pass

    httpd = HTTPServer((host, port), EntropyHandler)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()
