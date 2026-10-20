#!/bin/sh
# Validação completa (a mesma do CI): formatação, clippy e cobertura de 100% das linhas.
# Sem Rust local, use scripts/validar-docker.sh (Docker rust:latest).
set -e
rustup component add llvm-tools-preview rustfmt clippy >/dev/null 2>&1 || true
command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov --locked
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo llvm-cov --workspace --all-targets --ignore-filename-regex 'main\.rs' --fail-under-lines 100
