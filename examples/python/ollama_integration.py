#!/usr/bin/env python3
"""Feed hardware entropy to Ollama via a named pipe (FIFO).

This creates a named pipe at /tmp/openentropy-rng, fills it with
hardware entropy, and shows how to start Ollama with OLLAMA_AUXRNG_DEV.

Usage:
    # Terminal 1: Run this script
    python examples/python/ollama_integration.py

    # Terminal 2: Start Ollama with hardware entropy
    OLLAMA_AUXRNG_DEV=/tmp/openentropy-rng ollama run llama3
"""

import os
import signal
import sys

from openentropy import EntropyPool

FIFO_PATH = "/tmp/openentropy-rng"
CHUNK_SIZE = 4096

def cleanup(*_):
    """Remove the FIFO on exit."""
    if os.path.exists(FIFO_PATH):
        os.unlink(FIFO_PATH)
    sys.exit(0)

signal.signal(signal.SIGINT, cleanup)
signal.signal(signal.SIGTERM, cleanup)

# Create the named pipe
if os.path.exists(FIFO_PATH):
    os.unlink(FIFO_PATH)
os.mkfifo(FIFO_PATH)

print(f"Created entropy FIFO at {FIFO_PATH}")
print(f"Run Ollama with:")
print(f"  OLLAMA_AUXRNG_DEV={FIFO_PATH} ollama run llama3")
print(f"\nWaiting for reader... (Ctrl+C to stop)")

pool = EntropyPool.auto()

try:
    while True:
        # open() blocks until a reader connects
        with open(FIFO_PATH, "wb") as fifo:
            print("Reader connected — streaming entropy")
            try:
                while True:
                    data = pool.get_random_bytes(CHUNK_SIZE)
                    fifo.write(data)
                    fifo.flush()
            except BrokenPipeError:
                print("Reader disconnected, waiting for new reader...")
finally:
    cleanup()
