set dotenv-load

_default:
    @just --list

format:
    @cargo fmt

lint: format
    @cargo clippy --all-targets --all-features -- -D warnings --no-deps

build:
    cargo build

# Test poem-openapi with the client features. Uses the system linker because
# the rust-lld wrapper in the flox toolchain cache points at a stale path.
# Extra arguments are passed to `cargo test`, e.g. `just claude-test --test client_transport`.
claude-test *args:
    RUSTFLAGS="-Clinker-features=-lld" RUSTDOCFLAGS="-Clinker-features=-lld" cargo test -p poem-openapi --features client-reqwest {{args}}
