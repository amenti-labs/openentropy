"""Tests for NumPy Generator compatibility."""

import numpy as np
import pytest

from esoteric_entropy.numpy_compat import EsotericBitGenerator, EsotericGenerator


@pytest.fixture
def rng():
    """Create an EsotericGenerator with only clock jitter (fast)."""
    from esoteric_entropy.pool import EntropyPool
    from esoteric_entropy.sources.timing import ClockJitterSource
    pool = EntropyPool()
    pool.add_source(ClockJitterSource())
    bg = EsotericBitGenerator(pool=pool)
    return EsotericGenerator(bit_generator=bg)


def test_creation(rng):
    assert rng is not None
    assert rng.bit_generator is not None


def test_random_floats(rng):
    vals = rng.random(10)
    assert vals.shape == (10,)
    assert np.all(vals >= 0) and np.all(vals < 1)


def test_integers(rng):
    vals = rng.integers(0, 256, size=100)
    assert vals.shape == (100,)
    assert np.all(vals >= 0) and np.all(vals < 256)


def test_standard_normal(rng):
    vals = rng.standard_normal(1000)
    assert vals.shape == (1000,)
    assert abs(np.mean(vals)) < 0.5
    assert 0.5 < np.std(vals) < 2.0


def test_bytes(rng):
    data = rng.bytes(32)
    assert len(data) == 32
    assert isinstance(data, bytes)


def test_choice(rng):
    vals = rng.choice([1, 2, 3, 4, 5], size=10)
    assert len(vals) == 10
    assert all(v in [1, 2, 3, 4, 5] for v in vals)


def test_shuffle(rng):
    arr = np.arange(10)
    rng.shuffle(arr)
    assert set(arr) == set(range(10))


def test_permutation(rng):
    p = rng.permutation(10)
    assert set(p) == set(range(10))


def test_bit_generator_state(rng):
    state = rng.bit_generator.state
    assert state["bit_generator"] == "EsotericBitGenerator"
    assert "pool_sources" in state
