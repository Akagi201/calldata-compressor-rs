# calldata-compressor-rs

A Compression algorithm for EVM abi.encoded data, especially for EVM calldata.

This project is a Rust implementation of the Calldata Compressor, based on [1inch/calldata-compressor](https://github.com/1inch/calldata-compressor)

## Features

* Adhere to [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

## Test & Benchmark

Test

```sh
cargo nextest run -r
```

Benchmarks

```sh
# valgrind check
cargo valgrind test
valgrind --tool=dhat ./target/debug/deps/calldata_compressor-92413ab13fccdd8f
# generate flamegraph
cargo flamegraph --unit-test -- test_compress_big
```
