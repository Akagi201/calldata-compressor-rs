clippy:
  cargo +nightly clippy -- -W clippy::all -W clippy::pedantic -W clippy::nursery

build:
  cargo build

format:
  cargo +nightly fmt --all