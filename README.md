# wectr

## Build & run

```sh
git submodule update --init --recursive
cargo test --workspace
cargo build --release --manifest-path examples/bot/Cargo.toml \
  --target wasm32-unknown-unknown
cmake -S host -B build/host && cmake --build build/host -j
./build/host/wectr_host examples/bot/target/wasm32-unknown-unknown/release/bot.wasm
```

```
  move -> (1, 0)
  move -> (2, 0)
  move -> (3, 0)
  out of ammo
finished at (3, 0)
```
