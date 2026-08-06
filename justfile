set dotenv-load

_default:
    @just --list

format:
    @cargo fmt

lint: format
    @cargo clippy --all-targets --all-features -- -D warnings --no-deps

build:
    cargo build
