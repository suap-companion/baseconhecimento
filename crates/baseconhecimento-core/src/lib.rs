//! Biblioteca do `baseconhecimento`: espelha a Base de Conhecimento da Central de Serviços do SUAP
//! numa pasta local (Markdown + anexos) e a indexa para busca offline (SQLite/FTS5).
//!
//! Tudo o que é baixado fica na máquina, conforme o que a pessoa logada enxerga; nada daqui sai
//! para a rede além das requisições ao próprio SUAP.

pub mod clock;
pub mod crawl;
pub mod index;
pub mod markdown;
pub mod model;
pub mod parse;
pub mod sanitize;
pub mod schedule;
pub mod settings;
pub mod source;
pub mod store;

pub use model::{AttachmentRef, Entry, Visibility};
pub use suap_core;

/// Nome do programa para o `suap-core` (decide a pasta de dados e o prefixo das variáveis).
pub const APP_NAME: &str = "baseconhecimento";

/// Erros da biblioteca.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("erro de arquivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("arquivo de dados inválido: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Suap(#[from] suap_core::SuapError),
    #[error("índice de busca: {0}")]
    Index(#[from] suap_rag::RagError),
    #[error("{0}")]
    Settings(String),
    #[error("{0}")]
    Fetch(#[from] source::FetchError),
}

/// Dá o nome do programa ao `suap-core`; chame uma vez, na partida, antes de `AppPaths::discover`.
pub fn init() -> Result<(), Error> {
    Ok(suap_core::set_app_name(APP_NAME)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_app_once() {
        init().unwrap();
        init().unwrap();
        assert_eq!(suap_core::app_name(), APP_NAME);
    }

    #[test]
    fn errors_have_readable_messages() {
        let io: Error = std::io::Error::other("x").into();
        assert!(io.to_string().contains("erro de arquivo"));
        let json: Error = serde_json::from_str::<u8>("?").unwrap_err().into();
        assert!(json.to_string().contains("inválido"));
        assert!(Error::Settings("ruim".into()).to_string().contains("ruim"));
    }
}
