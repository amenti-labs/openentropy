"""Tests for openentropy PyO3 bindings.

Covers all 21 exported functions with at least 2 tests each:
  - Happy path: valid data, assert return type and key presence
  - Edge case: empty/minimal input, assert no crash

NOTE: These tests require `maturin develop` to have been run first.
All test data uses os.urandom() — no hardware entropy sources needed.
"""

import math
import os

import pytest

from openentropy import (
    full_analysis,
    autocorrelation_profile,
    spectral_analysis,
    bit_bias,
    distribution_stats,
    stationarity_test,
    runs_analysis,
    cross_correlation_matrix,
    pearson_correlation,
    compare,
    aggregate_delta,
    two_sample_tests,
    cliffs_delta,
    temporal_analysis,
    digram_analysis,
    markov_analysis,
    multi_lag_analysis,
    run_length_comparison,
    trial_analysis,
    stouffer_combine,
    calibration_check,
    benchmark_sources,
    bench_config_defaults,
    SessionWriter,
    record,
    list_sessions,
    load_session_meta,
    load_session_raw_data,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _random(n: int) -> bytes:
    """Return n bytes of pseudo-random data via os.urandom."""
    return os.urandom(n)


# ---------------------------------------------------------------------------
# Analysis bindings (9 functions)
# ---------------------------------------------------------------------------


class TestFullAnalysis:
    def test_returns_dict_with_expected_keys(self):
        data = _random(1000)
        result = full_analysis("test_source", data)
        assert isinstance(result, dict)
        for key in (
            "source_name",
            "sample_size",
            "shannon_entropy",
            "min_entropy",
            "autocorrelation",
            "spectral",
            "bit_bias",
            "distribution",
            "stationarity",
            "runs",
        ):
            assert key in result, f"Missing key: {key}"

    def test_entropy_in_valid_range(self):
        data = _random(1000)
        result = full_analysis("test", data)
        assert 0 <= result["shannon_entropy"] <= 8
        assert 0 <= result["min_entropy"] <= 8

    def test_empty_input(self):
        result = full_analysis("test", b"")
        assert isinstance(result, dict)


class TestAutocorrelationProfile:
    def test_returns_dict_with_expected_keys(self):
        data = _random(1000)
        result = autocorrelation_profile(data)
        assert isinstance(result, dict)
        for key in (
            "lags",
            "max_abs_correlation",
            "max_abs_lag",
            "threshold",
            "violations",
        ):
            assert key in result, f"Missing key: {key}"

    def test_single_byte(self):
        result = autocorrelation_profile(b"\xab")
        assert isinstance(result, dict)

    def test_custom_max_lag(self):
        data = _random(500)
        result = autocorrelation_profile(data, max_lag=16)
        assert isinstance(result, dict)
        assert len(result["lags"]) <= 16


class TestSpectralAnalysis:
    def test_returns_dict_with_expected_keys(self):
        data = _random(1024)
        result = spectral_analysis(data)
        assert isinstance(result, dict)
        for key in ("peaks", "flatness", "dominant_frequency", "total_power"):
            assert key in result, f"Missing key: {key}"

    def test_empty_input(self):
        result = spectral_analysis(b"")
        assert isinstance(result, dict)


class TestBitBias:
    def test_returns_dict_with_expected_keys(self):
        data = _random(1000)
        result = bit_bias(data)
        assert isinstance(result, dict)
        for key in (
            "bit_probabilities",
            "overall_bias",
            "chi_squared",
            "p_value",
            "has_significant_bias",
        ):
            assert key in result, f"Missing key: {key}"

    def test_single_byte(self):
        result = bit_bias(b"\xff")
        assert isinstance(result, dict)
        # All bits set — probabilities should all be 1.0
        assert all(p == 1.0 for p in result["bit_probabilities"])


class TestDistributionStats:
    def test_returns_dict_with_expected_keys(self):
        data = _random(1000)
        result = distribution_stats(data)
        assert isinstance(result, dict)
        for key in (
            "mean",
            "variance",
            "std_dev",
            "skewness",
            "kurtosis",
            "histogram",
            "ks_statistic",
            "ks_p_value",
        ):
            assert key in result, f"Missing key: {key}"

    def test_empty_input(self):
        result = distribution_stats(b"")
        assert isinstance(result, dict)


class TestStationarityTest:
    def test_returns_dict_with_expected_keys(self):
        data = _random(2000)
        result = stationarity_test(data)
        assert isinstance(result, dict)
        for key in (
            "is_stationary",
            "f_statistic",
            "window_means",
            "window_std_devs",
            "n_windows",
        ):
            assert key in result, f"Missing key: {key}"

    def test_small_input(self):
        result = stationarity_test(_random(10))
        assert isinstance(result, dict)


class TestRunsAnalysis:
    def test_returns_dict_with_expected_keys(self):
        data = _random(1000)
        result = runs_analysis(data)
        assert isinstance(result, dict)
        for key in (
            "longest_run",
            "expected_longest_run",
            "total_runs",
            "expected_runs",
        ):
            assert key in result, f"Missing key: {key}"

    def test_single_byte(self):
        result = runs_analysis(b"\x00")
        assert isinstance(result, dict)
        assert result["longest_run"] >= 1


class TestCrossCorrelationMatrix:
    def test_returns_dict_with_expected_keys(self):
        sources = [
            ("source_a", _random(500)),
            ("source_b", _random(500)),
            ("source_c", _random(500)),
        ]
        result = cross_correlation_matrix(sources)
        assert isinstance(result, dict)
        for key in ("pairs", "flagged_count"):
            assert key in result, f"Missing key: {key}"
        # 3 sources → 3 pairs (3 choose 2)
        assert len(result["pairs"]) == 3

    def test_empty_list(self):
        result = cross_correlation_matrix([])
        assert isinstance(result, dict)
        assert result["pairs"] == []
        assert result["flagged_count"] == 0


class TestPearsonCorrelation:
    def test_returns_float(self):
        a, b = _random(1000), _random(1000)
        r = pearson_correlation(a, b)
        assert isinstance(r, float)

    def test_range(self):
        a, b = _random(1000), _random(1000)
        r = pearson_correlation(a, b)
        assert -1 <= r <= 1

    def test_identical_data(self):
        data = _random(1000)
        r = pearson_correlation(data, data)
        assert isinstance(r, float)
        # Identical data → correlation should be 1.0 (or NaN if zero variance)
        assert r == pytest.approx(1.0) or math.isnan(r)


# ---------------------------------------------------------------------------
# Comparison bindings (9 functions)
# ---------------------------------------------------------------------------


class TestCompare:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(1000), _random(1000)
        result = compare("src_a", a, "src_b", b)
        assert isinstance(result, dict)
        for key in (
            "label_a",
            "label_b",
            "size_a",
            "size_b",
            "aggregate",
            "two_sample",
            "temporal",
            "digram",
            "markov",
            "multi_lag",
            "run_lengths",
        ):
            assert key in result, f"Missing key: {key}"

    def test_empty_inputs(self):
        result = compare("a", b"", "b", b"")
        assert isinstance(result, dict)
        assert result["size_a"] == 0
        assert result["size_b"] == 0


class TestAggregateDelta:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(1000), _random(1000)
        result = aggregate_delta(a, b)
        assert isinstance(result, dict)
        for key in (
            "shannon_a",
            "shannon_b",
            "min_entropy_a",
            "min_entropy_b",
            "mean_a",
            "mean_b",
            "cohens_d",
        ):
            assert key in result, f"Missing key: {key}"

    def test_identical_data(self):
        data = _random(500)
        result = aggregate_delta(data, data)
        assert isinstance(result, dict)
        # Same data → Cohen's d should be ~0
        assert result["cohens_d"] == pytest.approx(0.0, abs=0.01)


class TestTwoSampleTests:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(1000), _random(1000)
        result = two_sample_tests(a, b)
        assert isinstance(result, dict)
        for key in (
            "ks_statistic",
            "ks_p_value",
            "chi2_homogeneity",
            "chi2_df",
            "chi2_p_value",
            "chi2_reliable",
            "cliffs_delta",
            "mann_whitney_u",
            "mann_whitney_p_value",
        ):
            assert key in result, f"Missing key: {key}"

    def test_empty_inputs(self):
        result = two_sample_tests(b"", b"")
        assert isinstance(result, dict)


class TestCliffsDelta:
    def test_returns_float(self):
        a, b = _random(500), _random(500)
        d = cliffs_delta(a, b)
        assert isinstance(d, float)

    def test_range(self):
        a, b = _random(500), _random(500)
        d = cliffs_delta(a, b)
        assert -1 <= d <= 1

    def test_identical_data(self):
        data = _random(500)
        d = cliffs_delta(data, data)
        assert d == pytest.approx(0.0, abs=0.01)


class TestTemporalAnalysis:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(5000), _random(5000)
        result = temporal_analysis(a, b)
        assert isinstance(result, dict)
        for key in (
            "window_size",
            "anomaly_count_a",
            "anomaly_count_b",
            "max_z_a",
            "max_z_b",
            "top_anomalies_a",
            "top_anomalies_b",
            "windowed_entropy_a",
            "windowed_entropy_b",
        ):
            assert key in result, f"Missing key: {key}"

    def test_small_input(self):
        result = temporal_analysis(_random(10), _random(10))
        assert isinstance(result, dict)


class TestDigramAnalysis:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(1000), _random(1000)
        result = digram_analysis(a, b)
        assert isinstance(result, dict)
        for key in ("chi2_a", "chi2_b", "sufficient_data", "min_sample_bytes"):
            assert key in result, f"Missing key: {key}"

    def test_empty_inputs(self):
        result = digram_analysis(b"", b"")
        assert isinstance(result, dict)
        assert result["sufficient_data"] is False


class TestMarkovAnalysis:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(1000), _random(1000)
        result = markov_analysis(a, b)
        assert isinstance(result, dict)
        for key in ("transitions_a", "transitions_b"):
            assert key in result, f"Missing key: {key}"
        # 8 bits × 2×2 transition matrix
        assert len(result["transitions_a"]) == 8

    def test_single_byte(self):
        result = markov_analysis(b"\x00", b"\xff")
        assert isinstance(result, dict)


class TestMultiLagAnalysis:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(1000), _random(1000)
        result = multi_lag_analysis(a, b)
        assert isinstance(result, dict)
        for key in ("lags", "autocorr_a", "autocorr_b"):
            assert key in result, f"Missing key: {key}"

    def test_small_input(self):
        result = multi_lag_analysis(_random(5), _random(5))
        assert isinstance(result, dict)


class TestRunLengthComparison:
    def test_returns_dict_with_expected_keys(self):
        a, b = _random(1000), _random(1000)
        result = run_length_comparison(a, b)
        assert isinstance(result, dict)
        for key in ("distribution_a", "distribution_b"):
            assert key in result, f"Missing key: {key}"

    def test_empty_inputs(self):
        result = run_length_comparison(b"", b"")
        assert isinstance(result, dict)


# ---------------------------------------------------------------------------
# Trials bindings (3 functions)
# ---------------------------------------------------------------------------


class TestTrialAnalysis:
    def test_returns_dict_with_expected_keys(self):
        data = _random(2500)
        result = trial_analysis(data)
        assert isinstance(result, dict)
        for key in (
            "config",
            "bytes_consumed",
            "num_trials",
            "bits_per_trial",
            "terminal_cumulative_deviation",
            "terminal_z",
            "effect_size",
            "mean_z",
            "std_z",
            "terminal_p_value",
        ):
            assert key in result, f"Missing key: {key}"
        # "trials" key may be absent (skip_serializing_if = "Vec::is_empty")

    def test_num_trials_calculation(self):
        """2500 bytes / 25 bytes-per-trial (200 bits) = 100 trials."""
        data = _random(2500)
        result = trial_analysis(data)
        assert result["num_trials"] == 100
        assert result["bits_per_trial"] == 200
        assert result["bytes_consumed"] == 2500

    def test_custom_bits_per_trial(self):
        data = _random(1000)
        result = trial_analysis(data, bits_per_trial=80)
        assert isinstance(result, dict)
        # 80 bits = 10 bytes per trial → 1000/10 = 100 trials
        assert result["num_trials"] == 100
        assert result["bits_per_trial"] == 80

    def test_p_value_range(self):
        data = _random(2500)
        result = trial_analysis(data)
        assert 0 <= result["terminal_p_value"] <= 1


class TestStoufferCombine:
    def test_returns_dict_with_expected_keys(self):
        t1 = trial_analysis(_random(2500))
        result = stouffer_combine([t1])
        assert isinstance(result, dict)
        for key in (
            "num_sessions",
            "session_z_scores",
            "stouffer_z",
            "p_value",
            "combined_effect_size",
            "total_trials",
        ):
            assert key in result, f"Missing key: {key}"

    def test_round_trip(self):
        """Call trial_analysis twice, pass results to stouffer_combine."""
        t1 = trial_analysis(_random(2500))
        t2 = trial_analysis(_random(2500))
        result = stouffer_combine([t1, t2])
        assert result["num_sessions"] == 2
        assert result["total_trials"] == 200
        assert len(result["session_z_scores"]) == 2
        assert 0 <= result["p_value"] <= 1

    def test_empty_list(self):
        result = stouffer_combine([])
        assert isinstance(result, dict)
        assert result["num_sessions"] == 0
        assert result["total_trials"] == 0


class TestCalibrationCheck:
    def test_returns_dict_with_expected_keys(self):
        data = _random(5000)
        result = calibration_check(data)
        assert isinstance(result, dict)
        for key in (
            "analysis",
            "is_suitable",
            "warnings",
            "shannon_entropy",
            "bit_bias",
        ):
            assert key in result, f"Missing key: {key}"
        assert isinstance(result["is_suitable"], bool)
        assert isinstance(result["warnings"], list)

    def test_small_input(self):
        result = calibration_check(_random(10))
        assert isinstance(result, dict)
        # Very small input likely unsuitable
        assert isinstance(result["is_suitable"], bool)


# ---------------------------------------------------------------------------
# Cross-cutting sanity checks
# ---------------------------------------------------------------------------


class TestValueSanity:
    """Sanity checks on value ranges for random data."""

    def test_full_analysis_entropy_bounds(self):
        """Shannon and min entropy should be in [0, 8] for byte data."""
        result = full_analysis("sanity", _random(10_000))
        assert 0 <= result["shannon_entropy"] <= 8
        assert 0 <= result["min_entropy"] <= 8
        # For 10K random bytes, entropy should be high (> 7)
        assert result["shannon_entropy"] > 7.0

    def test_bit_bias_p_value_range(self):
        result = bit_bias(_random(1000))
        assert 0 <= result["p_value"] <= 1

    def test_distribution_stats_mean_near_expected(self):
        """For uniform random bytes, mean ≈ 127.5."""
        result = distribution_stats(_random(10_000))
        assert 100 < result["mean"] < 156  # generous bounds

    def test_aggregate_delta_entropy_bounds(self):
        result = aggregate_delta(_random(1000), _random(1000))
        assert 0 <= result["shannon_a"] <= 8
        assert 0 <= result["shannon_b"] <= 8

    def test_two_sample_p_values_range(self):
        result = two_sample_tests(_random(1000), _random(1000))
        assert 0 <= result["ks_p_value"] <= 1
        assert 0 <= result["chi2_p_value"] <= 1
        assert 0 <= result["mann_whitney_p_value"] <= 1

    def test_calibration_entropy_range(self):
        result = calibration_check(_random(5000))
        assert 0 <= result["shannon_entropy"] <= 8


# ---------------------------------------------------------------------------
# Benchmark bindings (2 functions)
# ---------------------------------------------------------------------------


class TestBenchmark:
    def test_bench_config_defaults_returns_dict(self):
        result = bench_config_defaults()
        assert isinstance(result, dict)
        assert "samples_per_round" in result
        assert "rounds" in result
        assert "warmup_rounds" in result
        assert "timeout_sec" in result

    def test_benchmark_sources_returns_report(self):
        from openentropy import EntropyPool

        pool = EntropyPool.auto()
        result = benchmark_sources(
            pool,
            {
                "rounds": 1,
                "warmup_rounds": 0,
                "samples_per_round": 64,
                "timeout_sec": 1.0,
            },
        )
        assert isinstance(result, dict)
        assert "sources" in result
        assert isinstance(result["sources"], list)


# ---------------------------------------------------------------------------
# Record bindings (SessionWriter + record)
# ---------------------------------------------------------------------------


class TestRecord:
    def test_session_writer_creates_and_finishes(self, tmp_path):
        from openentropy import EntropyPool

        pool = EntropyPool.auto()
        sources = [s["name"] for s in pool.sources()[:1]]
        if not sources:
            pytest.skip("no sources available")
        writer = SessionWriter(sources, str(tmp_path), conditioning="raw")
        writer.write_sample(sources[0], b"\x01\x02\x03", b"\x01\x02\x03")
        path = writer.finish()
        assert isinstance(path, str)
        import os

        assert os.path.isdir(path)
        assert os.path.exists(os.path.join(path, "session.json"))

    def test_session_writer_double_finish_raises(self, tmp_path):
        from openentropy import EntropyPool

        pool = EntropyPool.auto()
        sources = [s["name"] for s in pool.sources()[:1]]
        if not sources:
            pytest.skip("no sources available")
        writer = SessionWriter(sources, str(tmp_path), conditioning="raw")
        writer.finish()
        with pytest.raises(Exception, match="already finished"):
            writer.finish()

    def test_record_returns_dict(self, tmp_path):
        from openentropy import EntropyPool

        pool = EntropyPool.auto()
        sources = [s["name"] for s in pool.sources()[:1]]
        if not sources:
            pytest.skip("no sources available")
        result = record(
            pool,
            sources,
            duration_secs=0.5,
            conditioning="raw",
            output_dir=str(tmp_path),
        )
        assert isinstance(result, dict)
        assert "id" in result


# ---------------------------------------------------------------------------
# Sessions bindings (3 functions)
# ---------------------------------------------------------------------------


class TestSessions:
    def test_list_sessions_empty_dir(self, tmp_path):
        result = list_sessions(str(tmp_path))
        assert isinstance(result, list)
        assert len(result) == 0

    def test_list_sessions_nonexistent_dir(self, tmp_path):
        result = list_sessions(str(tmp_path / "nonexistent"))
        assert isinstance(result, list)
        assert len(result) == 0

    def test_load_session_meta_and_raw_data(self, tmp_path):
        from openentropy import EntropyPool

        pool = EntropyPool.auto()
        sources = [s["name"] for s in pool.sources()[:1]]
        if not sources:
            pytest.skip("no sources available")
        # Create a session first
        writer = SessionWriter(sources, str(tmp_path), conditioning="raw")
        writer.write_sample(sources[0], b"\xaa\xbb\xcc", b"\xaa\xbb\xcc")
        session_path = writer.finish()
        # Now test load functions
        meta = load_session_meta(session_path)
        assert isinstance(meta, dict)
        assert "id" in meta
        raw = load_session_raw_data(session_path)
        assert isinstance(raw, dict)


class TestDispatcher:
    def test_analysis_config_default(self):
        from openentropy import analysis_config

        config = analysis_config()
        assert isinstance(config, dict)
        assert config["forensic"] is True
        assert config["entropy"] is False
        assert config["chaos"] is False
        assert config["cross_correlation"] is False

    def test_analysis_config_deep(self):
        from openentropy import analysis_config

        config = analysis_config("deep")
        assert config["forensic"] is True
        assert config["entropy"] is True
        assert config["chaos"] is True
        assert config["cross_correlation"] is True
        assert config["trials"] == {"bits_per_trial": 200}

    def test_analysis_config_security(self):
        from openentropy import analysis_config

        config = analysis_config("security")
        assert config["entropy"] is True
        assert config["chaos"] is False
        assert config["trials"] is None

    def test_analyze_forensic_only(self):
        from openentropy import analyze

        data = bytes(range(256)) * 4
        report = analyze([("test", data)])
        assert isinstance(report, dict)
        assert len(report["sources"]) == 1
        src = report["sources"][0]
        assert src["label"] == "test"
        assert "forensic" in src
        assert "chaos" not in src
        assert "entropy" not in src
        assert "trials" not in src
        assert "autocorrelation" in src["verdicts"]

    def test_analyze_with_profile(self):
        from openentropy import analyze

        data = bytes(range(256)) * 4
        report = analyze([("test", data)], profile="deep")
        src = report["sources"][0]
        assert "forensic" in src
        assert "chaos" in src
        assert "entropy" in src
        assert "trials" in src
        assert "hurst" in src["verdicts"]

    def test_analyze_with_config_dict(self):
        from openentropy import analyze

        data = bytes(range(256)) * 4
        report = analyze(
            [("test", data)],
            config={"forensic": False, "chaos": True},
        )
        src = report["sources"][0]
        assert "forensic" not in src
        assert "chaos" in src

    def test_analyze_cross_correlation(self):
        from openentropy import analyze

        data = bytes(range(256)) * 4
        report = analyze(
            [("a", data), ("b", data)],
            config={"cross_correlation": True},
        )
        assert len(report["sources"]) == 2
        assert "cross_correlation" in report

    def test_analyze_cross_correlation_single_source(self):
        from openentropy import analyze

        data = bytes(range(256)) * 4
        report = analyze(
            [("a", data)],
            config={"cross_correlation": True},
        )
        assert "cross_correlation" not in report

    def test_analyze_serializable(self):
        import json
        from openentropy import analyze

        data = bytes(range(256)) * 4
        report = analyze([("test", data)], profile="quick")
        json_str = json.dumps(report)
        assert '"forensic"' in json_str
