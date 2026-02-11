"""Tests for conditioning algorithms."""

import numpy as np

from esoteric_entropy.conditioning import sha256_condition, von_neumann_debias, xor_fold


class TestVonNeumann:
    def test_removes_bias(self):
        # 70% zeros input
        bits = np.array([0] * 7000 + [1] * 3000, dtype=np.uint8)
        np.random.shuffle(bits)
        out, info = von_neumann_debias(bits)
        if len(out) > 50:
            bias = abs(np.mean(out) - 0.5)
            assert bias < 0.05  # should be close to 0.5

    def test_output_smaller(self):
        bits = np.random.randint(0, 2, 1000, dtype=np.uint8)
        out, info = von_neumann_debias(bits)
        assert len(out) < len(bits)
        assert info["efficiency"] > 0


class TestXorFold:
    def test_halves_length(self):
        data = np.arange(100, dtype=np.uint8)
        out = xor_fold(data, 2)
        assert len(out) == 50

    def test_fold_4(self):
        data = np.arange(100, dtype=np.uint8)
        out = xor_fold(data, 4)
        assert len(out) == 25


class TestSHA256:
    def test_output_length(self):
        data = np.random.randint(0, 256, 100, dtype=np.uint8)
        out = sha256_condition(data, 32)
        assert len(out) == 32

    def test_deterministic(self):
        data = b"test input"
        assert sha256_condition(data, 32) == sha256_condition(data, 32)

    def test_different_inputs(self):
        assert sha256_condition(b"a", 32) != sha256_condition(b"b", 32)
