# Imagem local de validação (sem Rust instalado): rust:latest com rustfmt, clippy e cargo-llvm-cov.
FROM rust:latest
RUN rustup component add llvm-tools-preview rustfmt clippy && cargo install cargo-llvm-cov --locked
