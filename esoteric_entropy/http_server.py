"""HTTP entropy server — ANU QRNG API compatible.

Serves random bytes via HTTP, compatible with quantum-llama.cpp's
QRNG backend and any client expecting the ANU API format.
"""

from __future__ import annotations

import json
import struct
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlparse

if TYPE_CHECKING:
    from esoteric_entropy.pool import EntropyPool


def _make_handler(pool: EntropyPool):
    """Create request handler with pool reference."""

    class EntropyHandler(BaseHTTPRequestHandler):
        _pool = pool

        def do_GET(self) -> None:
            parsed = urlparse(self.path)
            path = parsed.path.rstrip("/")
            params = parse_qs(parsed.query)

            if path == "/api/v1/random":
                self._handle_random(params)
            elif path == "/health":
                self._handle_health()
            elif path == "/sources":
                self._handle_sources()
            elif path == "/pool/status":
                self._handle_pool_status()
            else:
                self._json_response(404, {"error": "not found"})

        def _handle_random(self, params: dict) -> None:
            """ANU QRNG compatible random endpoint."""
            length = int(params.get("length", [1024])[0])
            data_type = params.get("type", ["hex16"])[0]
            length = max(1, min(length, 65536))  # cap at 64KB

            raw = self._pool.get_random_bytes(length)

            if data_type == "hex16":
                data = [raw[i:i+2].hex() for i in range(0, len(raw) - 1, 2)]
            elif data_type == "uint8":
                data = list(raw)
            elif data_type == "uint16":
                pairs = len(raw) // 2
                data = [struct.unpack("<H", raw[i*2:(i+1)*2])[0] for i in range(pairs)]
            else:
                data = raw.hex()

            result = {
                "type": data_type,
                "length": len(data) if isinstance(data, list) else length,
                "data": data,
                "success": True,
            }
            self._json_response(200, result)

        def _handle_health(self) -> None:
            report = self._pool.health_report()
            self._json_response(200, {
                "status": "healthy" if report["healthy"] > 0 else "degraded",
                "sources_healthy": report["healthy"],
                "sources_total": report["total"],
                "raw_bytes": report["raw_bytes"],
                "output_bytes": report["output_bytes"],
            })

        def _handle_sources(self) -> None:
            report = self._pool.health_report()
            self._json_response(200, {
                "sources": report["sources"],
                "total": report["total"],
            })

        def _handle_pool_status(self) -> None:
            report = self._pool.health_report()
            self._json_response(200, report)

        def _json_response(self, code: int, data: dict) -> None:
            body = json.dumps(data).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, format, *args):
            """Suppress default logging — use our format."""
            pass

    return EntropyHandler


def run_server(pool: EntropyPool, host: str = "127.0.0.1", port: int = 8042) -> None:
    """Run the entropy HTTP server."""
    handler = _make_handler(pool)
    server = HTTPServer((host, port), handler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
