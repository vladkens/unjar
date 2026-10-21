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

.PHONY: docs

docs: # the library docs as docs.rs builds them, opened in the browser
	RUSTDOCFLAGS="-D warnings" cargo doc --lib --no-default-features --locked --no-deps --open
