"""Tests for the CLI."""

from click.testing import CliRunner

from esoteric_entropy.cli import main


class TestCLI:
    def test_version(self):
        r = CliRunner().invoke(main, ["--version"])
        assert r.exit_code == 0
        assert "0.1.0" in r.output

    def test_scan(self):
        r = CliRunner().invoke(main, ["scan"])
        assert r.exit_code == 0
        assert "Platform" in r.output

    def test_probe_clock(self):
        r = CliRunner().invoke(main, ["probe", "clock"])
        assert r.exit_code == 0
        assert "Grade" in r.output

    def test_pool(self):
        r = CliRunner().invoke(main, ["pool"])
        assert r.exit_code == 0
        assert "Shannon entropy" in r.output
