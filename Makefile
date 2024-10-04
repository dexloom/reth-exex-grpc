# Define the RUSTFLAGS to treat warnings as errors
RELEASEFLAGS = -D warnings

# Target to run all tests
.PHONY: build
build:
	cargo build --all

release:
	RUSTFLAGS="-C target-cpu=native" cargo build --profile maxperf --features jemalloc,asm-keccak

# Target to run all tests
.PHONY: test
test:
	cargo test

# Target to run all benchmarks
.PHONY: clean
clean:
	cargo clean

# Target to run all benchmarks
.PHONY: bench
bench:
	cargo bench

# Target to run cargo clippy
.PHONY: clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

# format loom
.PHONY: fmt
fmt:
	cargo +stable fmt --all

# check files format fmt
.PHONY: fmt-check
fmt-check:
	cargo +stable fmt --all --check

# check licences
.PHONY: deny-check
deny-check:
	cargo deny --all-features check