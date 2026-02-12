"""Tests for the HTTP entropy server."""

import json
import threading
import time
import urllib.request

import pytest

from esoteric_entropy.pool import EntropyPool
from esoteric_entropy.sources.timing import ClockJitterSource


def _fast_pool():
    pool = EntropyPool()
    pool.add_source(ClockJitterSource())
    return pool


@pytest.fixture(scope="module")
def server_url():
    """Start server in background thread, yield URL, then shut down."""
    from http.server import BaseHTTPRequestHandler, HTTPServer
    from urllib.parse import parse_qs, urlparse

    pool = _fast_pool()
    port = 18042

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            parsed = urlparse(self.path)
            path = parsed.path.rstrip("/")
            params = parse_qs(parsed.query)

            if path == "/health":
                body = json.dumps({"status": "healthy"}).encode()
            elif path == "/api/v1/random":
                length = int(params.get("length", [16])[0])
                data = pool.get_random_bytes(min(length, 1024))
                dtype = params.get("type", ["uint8"])[0]
                if dtype == "uint8":
                    result = list(data)
                else:
                    result = [data[i:i+2].hex() for i in range(0, len(data)-1, 2)]
                body = json.dumps({"success": True, "data": result, "length": len(result)}).encode()
            else:
                self.send_response(404)
                self.end_headers()
                return

            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *a):
            pass

    httpd = HTTPServer(("127.0.0.1", port), Handler)
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    time.sleep(0.3)
    yield f"http://127.0.0.1:{port}"
    httpd.shutdown()


def test_health(server_url):
    resp = urllib.request.urlopen(f"{server_url}/health")
    data = json.loads(resp.read())
    assert data["status"] == "healthy"


def test_random_uint8(server_url):
    resp = urllib.request.urlopen(f"{server_url}/api/v1/random?length=32&type=uint8")
    data = json.loads(resp.read())
    assert data["success"] is True
    assert len(data["data"]) > 0
    assert all(0 <= v <= 255 for v in data["data"])


def test_random_hex16(server_url):
    resp = urllib.request.urlopen(f"{server_url}/api/v1/random?length=32&type=hex16")
    data = json.loads(resp.read())
    assert data["success"] is True
    assert len(data["data"]) > 0
