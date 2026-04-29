.PHONY: check build test lint fmt install \
        release-patch release-minor release-major \
        release-build release-archive homebrew-formula \
        build-wheel build-sdist publish-crates publish-pypi \
        test-live clean

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

# Cross-compile a release binary for $(TARGET).
# e.g. make release-build TARGET=x86_64-unknown-linux-gnu
release-build:
	@if [ -z "$(TARGET)" ]; then echo "TARGET is required"; exit 1; fi
	cargo build --release --target $(TARGET)

# Pack the binary built by release-build into a .tar.gz suitable for
# uploading to a GitHub Release. Drops the artifact in dist/.
# e.g. make release-archive TARGET=x86_64-unknown-linux-gnu VERSION=0.1.0
release-archive:
	@if [ -z "$(TARGET)" ]; then echo "TARGET is required"; exit 1; fi
	@if [ -z "$(VERSION)" ]; then echo "VERSION is required"; exit 1; fi
	mkdir -p dist
	tar -C target/$(TARGET)/release -czf dist/openarchieven-$(VERSION)-$(TARGET).tar.gz openarchieven
	cd dist && shasum -a 256 openarchieven-$(VERSION)-$(TARGET).tar.gz > openarchieven-$(VERSION)-$(TARGET).tar.gz.sha256

# Render the Homebrew formula from the .sha256 sidecar files in dist/.
# All four supported targets must already have an archive present.
# e.g. make homebrew-formula VERSION=0.1.0 TAG=v0.1.0
homebrew-formula:
	@if [ -z "$(VERSION)" ]; then echo "VERSION is required"; exit 1; fi
	@if [ -z "$(TAG)" ]; then echo "TAG is required"; exit 1; fi
	./scripts/render-homebrew-formula.sh $(VERSION) $(TAG) > dist/openarchieven.rb

# Build a Python wheel via maturin for $(TARGET).
# MATURIN_ARGS lets the caller append e.g. "--compatibility manylinux_2_28 --zig"
# for cross-compiled Linux wheels. The same invocation also produces the release
# binary in target/$(TARGET)/release/, which `release-archive` then packages.
# e.g. make build-wheel TARGET=x86_64-unknown-linux-gnu \
#                       MATURIN_ARGS="--compatibility manylinux_2_28 --zig"
build-wheel:
	@if [ -z "$(TARGET)" ]; then echo "TARGET is required"; exit 1; fi
	maturin build --release --target $(TARGET) $(MATURIN_ARGS)

# Build a source distribution (.tar.gz) for PyPI.
build-sdist:
	maturin sdist

# Publish the crate to crates.io. Expects CARGO_REGISTRY_TOKEN in the env.
publish-crates:
	cargo publish --locked

# Upload all wheels and the sdist found under $(WHEEL_DIR) and $(SDIST_DIR) to
# PyPI. Expects TWINE_USERNAME=__token__ and TWINE_PASSWORD=<pypi token> in env.
# e.g. make publish-pypi WHEEL_DIR=artifacts/wheels SDIST_DIR=artifacts/sdist
publish-pypi:
	@if [ -z "$(WHEEL_DIR)" ]; then echo "WHEEL_DIR is required"; exit 1; fi
	@if [ -z "$(SDIST_DIR)" ]; then echo "SDIST_DIR is required"; exit 1; fi
	uv tool run twine upload $(WHEEL_DIR)/**/*.whl $(SDIST_DIR)/*.tar.gz

test-live:
	OPENARCHIEVEN_TEST_LIVE=1 cargo test --test live -- --nocapture

clean:
	cargo clean
	rm -rf dist
