.PHONY: prepare check test build update clean

LINT := uv run --script https://raw.githubusercontent.com/vladkens/lint/main/rust.py

prepare:
	cargo +nightly fmt
	$(LINT) fix
	cargo clippy --fix --all-targets --locked --allow-dirty -- -D warnings
	cargo check --release --locked
	cargo check --lib --no-default-features --locked

check:
	cargo +nightly fmt --check
	cargo clippy --all-targets --locked -- -D warnings
	$(LINT)
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
	cargo update

clean:
	cargo clean

.PHONY: docs

docs: # the library docs as docs.rs builds them, opened in the browser
	RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-default-features --locked --no-deps --open
