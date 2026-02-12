"""Tests for the device (named pipe) feature."""

import os
import stat
import tempfile

import pytest


def test_mkfifo_creation():
    """Test that we can create and detect a FIFO."""
    with tempfile.TemporaryDirectory() as tmpdir:
        path = os.path.join(tmpdir, "test-rng")
        os.mkfifo(path)
        assert os.path.exists(path)
        assert stat.S_ISFIFO(os.stat(path).st_mode)
        os.unlink(path)


def test_fifo_detection():
    """Test the _is_fifo helper."""
    from esoteric_entropy.cli import _is_fifo

    with tempfile.TemporaryDirectory() as tmpdir:
        fifo_path = os.path.join(tmpdir, "fifo")
        os.mkfifo(fifo_path)
        assert _is_fifo(fifo_path) is True

        file_path = os.path.join(tmpdir, "regular")
        with open(file_path, "w") as f:
            f.write("not a fifo")
        assert _is_fifo(file_path) is False

        assert _is_fifo("/nonexistent/path") is False
