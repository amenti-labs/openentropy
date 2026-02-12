"""Tests for the stream CLI command."""

from click.testing import CliRunner

from esoteric_entropy.cli import main


def test_stream_hex():
    runner = CliRunner()
    result = runner.invoke(main, ["stream", "--format", "hex", "--bytes", "32", "--sources", "clock_jitter"])
    assert result.exit_code == 0
    output = result.output.strip()
    if output:
        assert all(c in "0123456789abcdef" for c in output)


def test_stream_base64():
    runner = CliRunner()
    result = runner.invoke(main, ["stream", "--format", "base64", "--bytes", "32", "--sources", "clock_jitter"])
    assert result.exit_code == 0
    output = result.output.strip()
    if output:
        import base64
        base64.b64decode(output)
