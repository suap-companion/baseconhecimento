use suap_core::AppPaths;

// Invólucro fino, fora da exigência de cobertura (veja o README); a lógica vive em `lib.rs`.
fn main() {
    let code = match baseconhecimento_core::init().and_then(|()| Ok(AppPaths::discover()?)) {
        Ok(paths) => {
            let password = std::env::var(baseconhecimento_cli::PASSWORD_ENV).ok();
            baseconhecimento_cli::run(
                std::env::args_os(),
                &paths,
                password,
                &mut std::io::stdout(),
                &mut std::io::stderr(),
            )
        }
        Err(error) => {
            eprintln!("erro: {error}");
            1
        }
    };
    std::process::exit(code);
}
