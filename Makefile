.PHONY: install dev test lint format build py-build clean

install:
	pip install -e .

dev:
	pip install maturin pytest ruff

test:
	cargo test --workspace --exclude openentropy-python

lint:
	cargo clippy --workspace --exclude openentropy-python -- -D warnings

format:
	cargo fmt --all

build:
	cargo build --release --workspace --exclude openentropy-python

py-build:
	cd crates/openentropy-python && maturin build --release

clean:
	rm -rf dist/ build/ *.egg-info .ruff_cache .pytest_cache __pycache__
