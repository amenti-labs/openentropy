.PHONY: install dev test lint format build clean

install:
	pip install -e .

dev:
	pip install -e ".[dev]"

test:
	pytest tests/ -v

lint:
	ruff check esoteric_entropy/ tests/

format:
	ruff format esoteric_entropy/ tests/

build:
	python -m build

clean:
	rm -rf dist/ build/ *.egg-info .ruff_cache .pytest_cache __pycache__
