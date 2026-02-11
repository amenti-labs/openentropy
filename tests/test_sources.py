"""Tests for entropy sources."""

import numpy as np
import pytest

from esoteric_entropy.sources import ALL_SOURCES
from esoteric_entropy.sources.base import EntropySource
from esoteric_entropy.sources.timing import ClockJitterSource, SleepJitterSource


class TestBaseSource:
    def test_quick_shannon_uniform(self):
        data = np.arange(256, dtype=np.uint8)
        h = EntropySource._quick_shannon(data)
        assert 7.9 < h <= 8.0

    def test_quick_shannon_constant(self):
        data = np.zeros(100, dtype=np.uint8)
        h = EntropySource._quick_shannon(data)
        assert h == pytest.approx(0.0, abs=0.01)

    def test_quick_quality_returns_dict(self):
        data = np.random.randint(0, 256, 500, dtype=np.uint8)
        q = EntropySource._quick_quality(data, "test")
        assert "grade" in q
        assert "shannon_entropy" in q
        assert q["samples"] == 500


class TestClockJitter:
    def test_is_available(self):
        assert ClockJitterSource().is_available()

    def test_collect_returns_uint8(self):
        data = ClockJitterSource().collect(100)
        assert data.dtype == np.uint8
        assert len(data) == 100

    def test_entropy_quality(self):
        q = ClockJitterSource().entropy_quality()
        assert q["grade"] in "ABCDF"


class TestSleepJitter:
    def test_collect(self):
        data = SleepJitterSource().collect(50)
        assert data.dtype == np.uint8
        assert len(data) == 50


class TestAllSourcesMetadata:
    """Every source class must have required metadata."""

    @pytest.mark.parametrize("cls", ALL_SOURCES, ids=lambda c: c.name)
    def test_has_name(self, cls):
        assert isinstance(cls.name, str) and len(cls.name) > 0

    @pytest.mark.parametrize("cls", ALL_SOURCES, ids=lambda c: c.name)
    def test_has_description(self, cls):
        assert isinstance(cls.description, str)

    @pytest.mark.parametrize("cls", ALL_SOURCES, ids=lambda c: c.name)
    def test_is_available_returns_bool(self, cls):
        src = cls()
        result = src.is_available()
        assert isinstance(result, bool)
