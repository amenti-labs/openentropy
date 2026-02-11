"""Tests for the entropy pool."""


from esoteric_entropy.pool import EntropyPool
from esoteric_entropy.sources.timing import ClockJitterSource


class TestPool:
    def test_create_empty(self):
        pool = EntropyPool()
        assert len(pool.sources) == 0

    def test_add_source(self):
        pool = EntropyPool()
        pool.add_source(ClockJitterSource())
        assert len(pool.sources) == 1

    def test_get_random_bytes(self):
        pool = EntropyPool()
        pool.add_source(ClockJitterSource())
        data = pool.get_random_bytes(32)
        assert len(data) == 32
        assert isinstance(data, bytes)

    def test_output_varies(self):
        pool = EntropyPool()
        pool.add_source(ClockJitterSource())
        a = pool.get_random_bytes(32)
        b = pool.get_random_bytes(32)
        assert a != b

    def test_health_report(self):
        pool = EntropyPool()
        pool.add_source(ClockJitterSource())
        pool.collect_all()
        r = pool.health_report()
        assert r["total"] == 1
        assert r["raw_bytes"] > 0

    def test_auto(self):
        pool = EntropyPool.auto()
        assert len(pool.sources) > 0
