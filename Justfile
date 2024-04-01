clippy:
  cargo +nightly clippy -- -W clippy::all -W clippy::pedantic -W clippy::nursery

build:
  cargo build

test:
  cargo nextest run -r

format:
  cargo +nightly fmt --all
