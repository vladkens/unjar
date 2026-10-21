.PHONY: prepare check test build update clean

prepare:
	cargo +nightly fmt
	cargo clippy --fix --all-targets --locked --allow-dirty -- -D warnings
	cargo check --release --locked
	cargo check --lib --no-default-features --locked

check:
	cargo +nightly fmt --check
	cargo clippy --all-targets --locked -- -D warnings
	cargo check --release --locked
	cargo check --lib --no-default-features --locked

test:
	cargo test --locked
	cargo test --lib --no-default-features --locked

build:
	cargo build --release --locked
	ls -lh target/release/unjar

update:
	cargo upgrade -i

clean:
	cargo clean

.PHONY: dist docs

# The prebuilt binary as target/distrib/unjar-dev-aarch64-apple-darwin.tar.gz, from an Apple
# Silicon Mac. `unjar -V` must say the package version: the crate's on a release commit,
# `git describe`'s otherwise (rust-build sets it in Cargo.toml).
dist: # the release archive for CI (vladkens/actions/rust-build)
	cargo build --release --locked --target aarch64-apple-darwin
	v="unjar $$(cargo pkgid | sed 's/.*[#@]//')"; b="$$(target/aarch64-apple-darwin/release/unjar -V)"; \
		test "$$b" = "$$v" || { echo "unjar -V says '$$b', expected '$$v'" >&2; exit 1; }
	rm -rf target/distrib && mkdir -p target/distrib
	tar czf target/distrib/unjar-dev-aarch64-apple-darwin.tar.gz readme.md LICENSE \
		-C target/aarch64-apple-darwin/release unjar
	ls -lh target/distrib

docs: # the library docs as docs.rs builds them, opened in the browser
	RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-default-features --locked --no-deps --open
