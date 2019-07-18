#!/bin/bash

set -e

git clone https://github.com/stm32-rs/stm32-rs
cd stm32-rs
cargo install --force svd2rust
# cargo install form  # not needed here
rustup component add rustfmt
# pip install --user pyyaml  # already installed
cd svd
./extract.sh
cd ..
make patch CRATES=stm32h7
make -j$(nproc) svd2rust CRATES=stm32h7
cd ..

sed -i 's|/your/path/here/||' Cargo.toml