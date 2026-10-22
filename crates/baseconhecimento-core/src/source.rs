//! De onde vêm as páginas e os arquivos: o SUAP autenticado (ou um falso, nos testes).

use suap_core::{AppPaths, SuapClient, SuapConfig, SuapError};
use tokio::runtime::{Builder, Runtime};

use crate::Error;

/// Por que uma página ou arquivo não veio.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// A sessão expirou ou não existe: é preciso `login`.
    #[error("sessão do SUAP ausente ou expirada: execute `baseconhecimento login`")]
    NotAuthenticated,
    /// O SUAP negou ou não achou (403/404): a pessoa logada não enxerga isso.
    #[error("o SUAP não permite a esta pessoa ver este endereço (403/404)")]
    Inaccessible,
    #[error("falha ao buscar: {0}")]
    Other(String),
}

/// Busca páginas e arquivos pelo caminho (relativo ao endereço do SUAP).
pub trait Fetcher {
    fn page(&self, path: &str) -> Result<String, FetchError>;
    fn file(&self, path: &str) -> Result<Vec<u8>, FetchError>;
}

fn classify(error: SuapError) -> FetchError {
    match error {
        SuapError::NotAuthenticated => FetchError::NotAuthenticated,
        SuapError::Transport(text)
            if text.starts_with("unexpected status 403")
                || text.starts_with("unexpected status 404") =>
        {
            FetchError::Inaccessible
        }
        other => FetchError::Other(other.to_string()),
    }
}

/// O SUAP de verdade, com a sessão salva do perfil.
pub struct SuapSource {
    client: SuapClient,
    runtime: Runtime,
}

impl SuapSource {
    pub fn open(paths: &AppPaths, config: &SuapConfig) -> Result<Self, Error> {
        Ok(Self {
            client: SuapClient::open(paths, config)?,
            runtime: Builder::new_current_thread().enable_all().build()?,
        })
    }

    /// Entra no SUAP e salva a sessão.
    pub fn login(&self, username: &str, password: &str) -> Result<(), Error> {
        Ok(self
            .runtime
            .block_on(self.client.login(username, password))?)
    }

    /// Guarda a sessão a partir do cabeçalho `Cookie` de um login feito noutro lugar (a WebView do
    /// app): a senha nunca chega aqui. Devolve quantos cookies guardou.
    pub fn import_session_cookies(&self, header: &str) -> Result<usize, Error> {
        Ok(self
            .runtime
            .block_on(self.client.import_cookie_header(header))?)
    }

    /// Se a sessão salva ainda vale.
    pub fn is_authenticated(&self) -> Result<bool, Error> {
        Ok(self.runtime.block_on(self.client.is_authenticated())?)
    }
}

impl Fetcher for SuapSource {
    fn page(&self, path: &str) -> Result<String, FetchError> {
        self.runtime
            .block_on(self.client.fetch_page(path))
            .map_err(classify)
    }

    /// Só baixa arquivos do próprio SUAP: a sessão não é enviada a outro servidor.
    fn file(&self, path: &str) -> Result<Vec<u8>, FetchError> {
        let base = self.client.base_url();
        let same_host = base
            .join(path)
            .map(|url| url.host_str() == base.host_str() && url.port() == base.port())
            .unwrap_or(false);
        if !same_host {
            return Err(FetchError::Other("anexo fora do SUAP não é baixado".into()));
        }
        self.runtime
            .block_on(self.client.fetch_file(path))
            .map(|(bytes, _)| bytes)
            .map_err(classify)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    const LOGIN_FORM: &str =
        r#"<form><input type="hidden" name="csrfmiddlewaretoken" value="tok"></form>"#;

    fn paths(dir: &tempfile::TempDir) -> AppPaths {
        AppPaths::from_dirs(dir.path().join("config"), dir.path().join("data"))
    }

    fn config(server: &MockServer) -> SuapConfig {
        SuapConfig {
            base_url: format!("{}/", server.uri()).parse().unwrap(),
            ..SuapConfig::default()
        }
    }

    #[test]
    fn classifies_errors() {
        assert_eq!(
            classify(SuapError::NotAuthenticated),
            FetchError::NotAuthenticated
        );
        let denied = SuapError::Transport("unexpected status 403 Forbidden".into());
        assert_eq!(classify(denied), FetchError::Inaccessible);
        let missing = SuapError::Transport("unexpected status 404 Not Found".into());
        assert_eq!(classify(missing), FetchError::Inaccessible);
        let broken = SuapError::Transport("unexpected status 500".into());
        assert!(matches!(classify(broken), FetchError::Other(_)));
        assert!(FetchError::NotAuthenticated.to_string().contains("login"));
        assert!(FetchError::Inaccessible.to_string().contains("403"));
    }

    #[test]
    fn logs_in_fetches_pages_and_files() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let server = runtime.block_on(MockServer::start());
        let mount = |verb: &str, route: &str, response: ResponseTemplate| {
            runtime.block_on(
                Mock::given(method(verb))
                    .and(path(route))
                    .respond_with(response)
                    .mount(&server),
            );
        };
        mount(
            "GET",
            "/accounts/login/",
            ResponseTemplate::new(200).set_body_string(LOGIN_FORM),
        );
        mount(
            "POST",
            "/accounts/login/",
            ResponseTemplate::new(302).insert_header("location", "/"),
        );
        mount(
            "GET",
            "/",
            ResponseTemplate::new(200).set_body_string("inicio"),
        );
        mount(
            "GET",
            "/pagina/",
            ResponseTemplate::new(200).set_body_string("conteudo"),
        );
        mount(
            "GET",
            "/arquivo.bin",
            ResponseTemplate::new(200).set_body_bytes(vec![1u8, 2, 3]),
        );
        mount("GET", "/negado/", ResponseTemplate::new(403));
        let dir = tempfile::tempdir().unwrap();
        let source = SuapSource::open(&paths(&dir), &config(&server)).unwrap();
        source.login("fulano", "segredo").unwrap();
        assert!(source.is_authenticated().unwrap());
        assert_eq!(
            source
                .import_session_cookies("sessionid=abc; csrftoken=xyz")
                .unwrap(),
            2
        );
        assert_eq!(source.page("/pagina/").unwrap(), "conteudo");
        assert_eq!(source.file("/arquivo.bin").unwrap(), vec![1, 2, 3]);
        let absolute = format!("{}/arquivo.bin", server.uri());
        assert_eq!(source.file(&absolute).unwrap(), vec![1, 2, 3]);
        assert_eq!(source.page("/negado/"), Err(FetchError::Inaccessible));
        let foreign = source.file("https://outro.exemplo/x.pdf").unwrap_err();
        assert!(matches!(foreign, FetchError::Other(text) if text.contains("fora do SUAP")));
    }
}
