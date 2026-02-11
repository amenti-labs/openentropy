"""Tests for statistical test suite."""

import numpy as np

from esoteric_entropy.stats import (
    compression_ratio,
    full_report,
    min_entropy,
    shannon_entropy,
)


class TestShannon:
    def test_uniform(self):
        data = np.tile(np.arange(256, dtype=np.uint8), 10)
        h = shannon_entropy(data)
        assert 7.9 < h <= 8.0

    def test_constant(self):
        h = shannon_entropy(np.zeros(100, dtype=np.uint8))
        assert h < 0.01


class TestMinEntropy:
    def test_uniform(self):
        data = np.tile(np.arange(256, dtype=np.uint8), 10)
        h = min_entropy(data)
        assert h > 7.9

    def test_biased(self):
        data = np.array([0] * 900 + [1] * 100, dtype=np.uint8)
        h = min_entropy(data)
        assert h < 1.0


class TestCompression:
    def test_random_incompressible(self):
        data = np.random.randint(0, 256, 10000, dtype=np.uint8).tobytes()
        r = compression_ratio(data)
        assert r > 0.95

    def test_constant_compressible(self):
        data = bytes(10000)
        r = compression_ratio(data)
        assert r < 0.05


class TestFullReport:
    def test_returns_grade(self):
        data = np.random.randint(0, 256, 5000, dtype=np.uint8)
        r = full_report(data, "test")
        assert r["grade"] in "ABCDF"
        assert r["samples"] == 5000
