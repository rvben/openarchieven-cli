.PHONY: check build test lint fmt install release-patch release-minor release-major test-live clean

check: lint test

build:
	cargo build --release

test:
	cargo nextest run --no-fail-fast || cargo test --no-fail-fast

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

install:
	cargo build --release
	mkdir -p $(HOME)/.local/bin
	cp target/release/openarchieven $(HOME)/.local/bin/openarchieven

release-patch:
	vership bump patch

release-minor:
	vership bump minor

release-major:
	vership bump major

test-live:
	OPENARCHIEVEN_TEST_LIVE=1 cargo test --test live -- --nocapture

clean:
	cargo clean
