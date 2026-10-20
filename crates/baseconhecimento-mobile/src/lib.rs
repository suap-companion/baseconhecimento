//! Fachada do `baseconhecimento` para o aplicativo Android (Kotlin, via UniFFI).
//!
//! O objeto [`Base`] reúne o que o app faz sobre a biblioteca: sessão (cookies do login feito na
//! WebView; a senha nunca passa pelo app), leitura e busca offline da cópia local, configurações da
//! atualização e a atualização em si, com progresso e cancelamento. Tudo o que é baixado fica só na
//! pasta privada do app; nada daqui sai para a rede além das requisições ao próprio SUAP.
//!
//! Não há sincronização entre aparelhos: o conteúdo da Base de Conhecimento não sai da máquina.

uniffi::setup_scaffolding!();

mod reading;
mod updating;

use std::{fs, path::PathBuf, sync::Arc};

use baseconhecimento_core::{
    source::{FetchError, SuapSource},
    store::data_root,
    Error,
};
use suap_core::{AppPaths, SuapConfig, SuapError};

/// Erros da fachada, como o aplicativo os vê.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BaseError {
    /// A sessão do SUAP não existe ou expirou: o app volta ao login.
    #[error("sessão do SUAP ausente ou expirada: entre de novo")]
    SessionExpired,
    // O campo não se chama `message`: em Kotlin isso colidiria com `Throwable.message`.
    #[error("{detail}")]
    Failed { detail: String },
}

pub(crate) fn failed(error: impl std::fmt::Display) -> BaseError {
    BaseError::Failed {
        detail: error.to_string(),
    }
}

/// Traduz o erro da biblioteca; a sessão acabada ganha o seu próprio caso.
pub(crate) fn convert(error: Error) -> BaseError {
    match error {
        Error::Fetch(FetchError::NotAuthenticated) | Error::Suap(SuapError::NotAuthenticated) => {
            BaseError::SessionExpired
        }
        other => failed(other),
    }
}

/// Versão da biblioteca (prova de que ela carregou).
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Um perfil do SUAP no aparelho: a sessão e a cópia local de uma pasta.
#[derive(uniffi::Object)]
pub struct Base {
    paths: AppPaths,
    config: SuapConfig,
    root: PathBuf,
}

#[uniffi::export]
impl Base {
    /// Abre (ou cria) o armazenamento em `data_dir` para o SUAP em `base_url`.
    #[uniffi::constructor]
    pub fn new(data_dir: String, base_url: String) -> Result<Arc<Self>, BaseError> {
        let dir = PathBuf::from(data_dir);
        let paths = AppPaths::from_dirs(dir.join("config"), dir.join("data"));
        paths.ensure_dirs().map_err(failed)?;
        let config = SuapConfig {
            base_url: base_url.parse().map_err(failed)?,
            ..SuapConfig::default()
        };
        let root = data_root(&paths);
        Ok(Arc::new(Self {
            paths,
            config,
            root,
        }))
    }

    /// Recebe os cookies da WebView de login (o cabeçalho `Cookie`); devolve quantos guardou.
    pub fn import_session_cookies(&self, header: String) -> Result<u32, BaseError> {
        let kept = self.source()?.import_session_cookies(&header);
        Ok(kept.map_err(convert)? as u32)
    }

    /// Se há uma sessão guardada (sem rede; ela pode ter expirado).
    pub fn has_session(&self) -> bool {
        self.paths.session_file().exists()
    }

    /// Pergunta ao SUAP se a sessão guardada ainda vale.
    pub fn is_authenticated(&self) -> Result<bool, BaseError> {
        if !self.has_session() {
            return Ok(false);
        }
        self.source()?.is_authenticated().map_err(convert)
    }

    /// Esquece a sessão; a cópia local continua legível.
    pub fn logout(&self) -> Result<(), BaseError> {
        if self.has_session() {
            fs::remove_file(self.paths.session_file()).map_err(failed)?;
        }
        Ok(())
    }

    /// Apaga a cópia local (entradas, anexos, índice e configurações) deste perfil.
    pub fn delete_local_copy(&self) -> Result<(), BaseError> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root).map_err(failed)?;
        }
        Ok(())
    }
}

impl Base {
    fn source(&self) -> Result<SuapSource, BaseError> {
        SuapSource::open(&self.paths, &self.config).map_err(convert)
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    //! SUAP falso (wiremock) com dados sintéticos, para os testes da fachada.

    use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

    use tempfile::TempDir;
    use tokio::runtime::Runtime;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    use crate::Base;

    const LOGIN_FORM: &str =
        r#"<form><input type="hidden" name="csrfmiddlewaretoken" value="tok"></form>"#;
    pub const BASE: &str = "/centralservicos/baseconhecimento";

    /// As atualizações compartilham o portão do processo: os testes que as fazem rodam em fila.
    static SERIAL: Mutex<()> = Mutex::new(());

    pub fn serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub struct Env {
        runtime: Runtime,
        pub server: MockServer,
        pub dir: TempDir,
    }

    impl Env {
        pub fn new() -> Self {
            let runtime = Runtime::new().unwrap();
            let server = runtime.block_on(MockServer::start());
            Self {
                runtime,
                server,
                dir: tempfile::tempdir().unwrap(),
            }
        }

        pub fn base(&self) -> Arc<Base> {
            let data = self.dir.path().to_string_lossy().into_owned();
            Base::new(data, format!("{}/", self.server.uri())).unwrap()
        }

        pub fn mount(&self, verb: &str, route: &str, response: ResponseTemplate) {
            self.runtime.block_on(
                Mock::given(method(verb))
                    .and(path(route))
                    .respond_with(response)
                    .mount(&self.server),
            );
        }

        pub fn page(&self, route: &str, body: &str) {
            let response = ResponseTemplate::new(200).set_body_string(body.to_owned());
            self.mount("GET", route, response);
        }

        /// Login, uma área, três entradas (uma privada com anexo e imagem, uma pública, uma negada).
        pub fn suap(&self) {
            self.page("/accounts/login/", LOGIN_FORM);
            self.page("/", "inicio");
            self.page(
                &format!("{BASE}/"),
                &format!(
                    r#"<a class="box-item" href="{BASE}/dashboard/ti/"><h4>Tecnologia</h4></a>"#
                ),
            );
            self.page(
                &format!("{BASE}/dashboard/ti/"),
                &format!(
                    r#"<a href="{BASE}/1/">a</a><a href="{BASE}/2/">b</a><a href="{BASE}/3/">c</a>"#
                ),
            );
            self.page(
                &format!("{BASE}/1/"),
                r#"<h2>Trocar o toner</h2><span class="status status-privada">Privada</span>
                   <div id="resumo"><div class="accordion-body"><p>Procedimento para a impressora do setor.</p><img src="/media/toner.png" alt="toner"></div></div>
                   <div class="tab-pane" data-title="Anexos"><a href="/media/guia.txt">guia.txt</a></div>
                   <dl class="details-container"><dt>Área:</dt><dd>Tecnologia</dd></dl>
                   <ul class="tags"><li>impressora</li><li>toner</li></ul>
                   <ul><li>Última Atualização por Pessoa em 01/01/2026 10:00</li></ul>"#,
            );
            self.page(
                &format!("{BASE}/2/"),
                r#"<h2>Solicitar conta</h2><span class="status status-publica">Pública</span>
                   <div id="solucao"><div class="accordion-body"><p>Abra um chamado de acesso.</p></div></div>"#,
            );
            self.mount("GET", &format!("{BASE}/3/"), ResponseTemplate::new(403));
            self.mount(
                "GET",
                "/media/toner.png",
                ResponseTemplate::new(200).set_body_bytes(vec![1u8, 2]),
            );
            self.mount(
                "GET",
                "/media/guia.txt",
                ResponseTemplate::new(200).set_body_string("trocar o cartucho do toner"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Env;

    #[test]
    fn errors_say_what_happened() {
        let expired = convert(Error::Fetch(FetchError::NotAuthenticated));
        assert!(matches!(expired, BaseError::SessionExpired));
        assert!(expired.to_string().contains("entre de novo"));
        let suap = convert(Error::Suap(SuapError::NotAuthenticated));
        assert!(matches!(suap, BaseError::SessionExpired));
        let other = convert(Error::Settings("ruim".into()));
        assert_eq!(other.to_string(), "ruim");
        assert!(!core_version().is_empty());
    }

    #[test]
    fn rejects_a_bad_address_and_an_unusable_folder() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_string_lossy().into_owned();
        assert!(Base::new(data, "isto não é um endereço".into()).is_err());
        let file = dir.path().join("arquivo");
        fs::write(&file, "x").unwrap();
        let blocked = file.to_string_lossy().into_owned();
        assert!(Base::new(blocked, "https://suap.exemplo.br/".into()).is_err());
    }

    #[test]
    fn the_session_comes_from_cookies_and_goes_away_on_logout() {
        let env = Env::new();
        env.suap();
        let base = env.base();
        assert!(!base.has_session());
        assert!(!base.is_authenticated().unwrap());
        let kept = base
            .import_session_cookies("sessionid=abc; csrftoken=xyz".into())
            .unwrap();
        assert_eq!(kept, 2);
        assert!(base.has_session());
        assert!(base.is_authenticated().unwrap());
        base.logout().unwrap();
        assert!(!base.has_session());
        base.logout().unwrap();
    }

    #[test]
    fn deleting_the_local_copy_works_once_and_reports_failures() {
        let env = Env::new();
        let base = env.base();
        base.delete_local_copy().unwrap();
        fs::create_dir_all(&base.root).unwrap();
        fs::write(base.root.join("manifesto.json"), "{}").unwrap();
        base.delete_local_copy().unwrap();
        assert!(!base.root.exists());
        // Um arquivo no lugar da pasta não se apaga como pasta: o erro chega ao app.
        fs::write(&base.root, "x").unwrap();
        assert!(base.delete_local_copy().is_err());
    }
}
