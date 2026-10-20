//! Testes da CLI contra um SUAP falso (wiremock) com dados sintéticos.

use baseconhecimento_cli::{data_root, run, PASSWORD_ENV};
use suap_core::AppPaths;
use tempfile::TempDir;
use tokio::runtime::Runtime;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const LOGIN_FORM: &str =
    r#"<form><input type="hidden" name="csrfmiddlewaretoken" value="tok"></form>"#;
const BASE: &str = "/centralservicos/baseconhecimento";

struct Env {
    runtime: Runtime,
    server: MockServer,
    _dir: TempDir,
    paths: AppPaths,
}

impl Env {
    fn new() -> Self {
        let runtime = Runtime::new().unwrap();
        let server = runtime.block_on(MockServer::start());
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_dirs(dir.path().join("config"), dir.path().join("data"));
        Self {
            runtime,
            server,
            _dir: dir,
            paths,
        }
    }

    fn mount(&self, verb: &str, route: &str, response: ResponseTemplate) {
        self.runtime.block_on(
            Mock::given(method(verb))
                .and(path(route))
                .respond_with(response)
                .mount(&self.server),
        );
    }

    fn page(&self, route: &str, body: &str) {
        self.mount(
            "GET",
            route,
            ResponseTemplate::new(200).set_body_string(body.to_owned()),
        );
    }

    /// SUAP falso: login, duas áreas, três entradas (uma privada com anexo, uma pública, uma negada).
    fn suap(&self) {
        self.page("/accounts/login/", LOGIN_FORM);
        self.mount(
            "POST",
            "/accounts/login/",
            ResponseTemplate::new(302).insert_header("location", "/"),
        );
        self.page("/", "inicio");
        self.page(
            &format!("{BASE}/"),
            &format!(r#"<a class="box-item" href="{BASE}/dashboard/ti/"><h4>Tecnologia</h4></a>"#),
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
               <div id="resumo"><div class="accordion-body"><p>Procedimento para a impressora do setor.</p></div></div>
               <div class="tab-pane" data-title="Anexos"><a href="/media/guia.txt">guia.txt</a></div>
               <dl class="details-container"><dt>Área:</dt><dd>Tecnologia</dd></dl>
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
            "/media/guia.txt",
            ResponseTemplate::new(200).set_body_string("trocar o cartucho do toner"),
        );
    }

    fn cli(&self, args: &[&str]) -> (i32, String, String) {
        self.cli_with(args, Some("segredo"))
    }

    fn cli_with(&self, args: &[&str], password: Option<&str>) -> (i32, String, String) {
        let mut all = vec!["baseconhecimento"];
        all.extend_from_slice(args);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(
            all,
            &self.paths,
            password.map(str::to_owned),
            &mut out,
            &mut err,
        );
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn ok(&self, args: &[&str]) -> String {
        let (code, out, err) = self.cli(args);
        assert_eq!(code, 0, "{args:?}: {err}");
        out
    }

    fn fails(&self, args: &[&str]) -> String {
        let (code, _, err) = self.cli(args);
        assert_eq!(code, 1, "{args:?} deveria falhar");
        err
    }

    fn profile(&self) {
        let url = self.server.uri();
        self.ok(&[
            "profile",
            "init",
            "--base-url",
            &url,
            "--username",
            "fulano",
        ]);
    }
}

#[test]
fn manages_profiles() {
    let env = Env::new();
    assert!(env.fails(&["profile", "show"]).contains("não configurado"));
    assert!(env
        .fails(&["profile", "update", "--username", "x"])
        .contains("não configurado"));
    assert!(env.ok(&["profile", "list"]).is_empty());
    assert_eq!(env.ok(&["profile", "list", "--json"]).trim(), "[]");
    env.profile();
    assert!(env.fails(&["profile", "init"]).contains("já existe"));
    assert!(env.fails(&["profile", "update"]).contains("nada a alterar"));
    assert!(env
        .fails(&["profile", "init", "x", "--base-url", "ftp://x"])
        .contains("HTTP"));
    assert!(env
        .fails(&["profile", "init", "x", "--base-url", "não é url"])
        .contains("relative URL"));
    assert!(env
        .fails(&["--profile", "a b", "paths"])
        .contains("invalid profile"));
    env.ok(&[
        "profile",
        "update",
        "--username",
        "ciclano",
        "--base-url",
        "https://suap.exemplo.br/",
    ]);
    env.ok(&[
        "profile",
        "init",
        "outro",
        "--base-url",
        "https://outro.exemplo.br/",
    ]);
    assert!(env.ok(&["profile", "show"]).contains("usuário: ciclano"));
    assert!(env
        .ok(&["profile", "show", "--json"])
        .contains("\"usuario\":\"ciclano\""));
    assert!(env
        .ok(&["profile", "show", "outro"])
        .contains("perfil:  outro"));
    let list = env.ok(&["profile", "list"]);
    assert!(list.contains("default\thttps://suap.exemplo.br/\tciclano") && list.contains("outro"));
    assert!(env
        .ok(&["profile", "list", "--json"])
        .contains("\"perfil\":\"outro\""));
    let paths = env.ok(&["paths"]);
    assert!(paths.contains("perfil:      default") && paths.contains("cópia local"));
    assert!(env.fails(&["profile", "remove", "outro"]).contains("--yes"));
    assert!(env
        .fails(&["profile", "remove", "nenhum", "--yes"])
        .contains("não configurado"));
    std::fs::create_dir_all(data_root(&env.paths.clone().with_profile("outro").unwrap())).unwrap();
    env.ok(&["profile", "remove", "outro", "--yes", "--apagar-dados"]);
    assert!(!data_root(&env.paths.clone().with_profile("outro").unwrap()).exists());
    env.ok(&["profile", "remove", "default", "--yes"]);
}

#[test]
fn reports_usage_errors() {
    let env = Env::new();
    let (code, out, _) = env.cli(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("Base de Conhecimento"));
    let (code, _, err) = env.cli(&["buscar"]);
    assert_ne!(code, 0);
    assert!(err.contains("CONSULTA") || err.contains("consulta"));
}

#[test]
fn logs_in_and_checks_the_session() {
    let env = Env::new();
    env.suap();
    assert!(env.fails(&["login"]).contains("não configurado"));
    assert!(env.fails(&["session-status"]).contains("não configurado"));
    let url = env.server.uri();
    env.ok(&["profile", "init", "--base-url", &url]);
    assert!(env.fails(&["login"]).contains("usuário não informado"));
    let (code, _, err) = env.cli_with(&["login", "--username", "fulano"], None);
    assert_eq!(code, 1);
    assert!(err.contains(PASSWORD_ENV));
    assert!(env.fails(&["session-status"]).contains("nenhuma sessão"));
    let out = env.ok(&["login", "--username", "fulano"]);
    assert!(out.contains("Login realizado como fulano") && !out.contains("segredo"));
    assert!(env.ok(&["session-status"]).contains("Sessão válida"));
    env.ok(&["profile", "remove", "default", "--yes"]);
    assert!(!env.paths.session_file().exists());
}

#[test]
fn expired_session_is_reported() {
    let env = Env::new();
    env.page("/accounts/login/", LOGIN_FORM);
    env.mount(
        "GET",
        "/",
        ResponseTemplate::new(302).insert_header("location", "/accounts/login/"),
    );
    env.profile();
    std::fs::create_dir_all(env.paths.data_dir()).unwrap();
    std::fs::write(env.paths.session_file(), "[]").unwrap();
    assert!(env.fails(&["session-status"]).contains("expirou"));
}

#[test]
fn updates_lists_shows_and_searches() {
    let env = Env::new();
    env.suap();
    env.profile();
    assert!(env
        .fails(&["atualizar"])
        .contains("execute `baseconhecimento login`"));
    env.ok(&["login"]);

    let out = env.ok(&["atualizar"]);
    assert!(out.contains("Concluído: 2 nova(s), 0 atualizada(s), 0 sem mudança, 1 sem acesso, 0 falha(s); anexos: 1 baixado(s)"));
    let again = env.ok(&["atualizar"]);
    assert!(again.contains("0 nova(s), 0 atualizada(s), 2 sem mudança"));
    assert!(env
        .ok(&["atualizar", "--forcar", "--id", "1"])
        .contains("1 atualizada(s)"));
    assert!(env.ok(&["atualizar", "--quiet"]).is_empty());
    assert!(env
        .ok(&["atualizar", "--varrer", "--parada", "2"])
        .contains("0 nova(s)"));

    let listed = env.ok(&["listar"]);
    assert!(
        listed.contains("1\tprivada\tTecnologia\tTrocar o toner")
            && listed.contains("2\tpublica\t-\tSolicitar conta")
    );
    assert!(!env
        .ok(&["listar", "--visibilidade", "publica"])
        .contains("toner"));
    assert!(
        env.ok(&["listar", "--area", "tecno"]).contains("toner")
            && !env.ok(&["listar", "--area", "tecno"]).contains("conta")
    );
    assert!(env
        .ok(&["listar", "--json"])
        .contains("\"titulo\":\"Trocar o toner\""));
    assert!(env
        .fails(&["listar", "--visibilidade", "x"])
        .contains("visibilidade inválida"));

    let shown = env.ok(&["mostrar", "1"]);
    assert!(shown.starts_with("---\nid: 1\n") && shown.contains("## Resumo"));
    assert!(env.ok(&["mostrar", "1", "--json"]).contains("\"arquivo\""));
    assert!(env
        .fails(&["mostrar", "99"])
        .contains("não está na cópia local"));

    let found = env.ok(&["buscar", "impressora", "setor"]);
    assert!(found.contains("[1] Trocar o toner (privada; entrada)"));
    assert!(env
        .ok(&["buscar", "--limite", "1", "zzzinexistente"])
        .contains("Nada encontrado."));
    assert!(env
        .ok(&["buscar", "--json", "cartucho"])
        .contains("\"origem\":\"guia.txt\""));

    assert!(env.ok(&["reindexar"]).contains("2 entrada(s)"));
    let status = env.ok(&["status"]);
    assert!(
        status.contains("Entradas: 2 (1 públicas, 1 privadas, 0 sigilosas)")
            && status.contains("a cada 7 dia(s)")
    );
    assert!(env.ok(&["status", "--json"]).contains("\"privadas\":1"));
}

#[test]
fn honors_the_weekly_schedule_and_settings() {
    let env = Env::new();
    env.suap();
    env.profile();
    env.ok(&["login"]);
    let first = env.ok(&["atualizar", "--se-vencido"]);
    assert!(first.contains("2 nova(s)"));
    assert!(env
        .ok(&["atualizar", "--se-vencido"])
        .contains("Ainda não venceu"));
    assert!(env.ok(&["status"]).contains("Última atualização: 20"));
    env.ok(&[
        "config",
        "--auto-atualizar",
        "false",
        "--intervalo-dias",
        "1",
        "--anexos",
        "false",
    ]);
    assert!(env.ok(&["atualizar", "--se-vencido"]).contains("desligada"));
    let shown = env.ok(&["config"]);
    assert!(
        shown.contains("atualização automática: false")
            && shown.contains("intervalo (dias):       1")
    );
    assert!(env.ok(&["status"]).contains("desligada"));
    env.ok(&["config", "--auto-atualizar", "true"]);
    assert!(env
        .fails(&["config", "--intervalo-dias", "0"])
        .contains("entre 1 e 365"));
    std::fs::remove_file(data_root(&env.paths).join("settings.json")).unwrap();
    assert!(env.ok(&["status"]).contains("Atualização vencida"));
}

#[test]
fn warns_about_failures_and_denied_areas() {
    let env = Env::new();
    env.page("/accounts/login/", LOGIN_FORM);
    env.mount(
        "POST",
        "/accounts/login/",
        ResponseTemplate::new(302).insert_header("location", "/"),
    );
    env.page("/", "inicio");
    env.page(&format!("{BASE}/"), &format!(r#"<a class="box-item" href="{BASE}/dashboard/ti/"><h4>TI</h4></a><a class="box-item" href="{BASE}/dashboard/rh/"><h4>RH</h4></a>"#));
    env.page(
        &format!("{BASE}/dashboard/ti/"),
        &format!(r#"<a href="{BASE}/5/">a</a>"#),
    );
    env.mount(
        "GET",
        &format!("{BASE}/dashboard/rh/"),
        ResponseTemplate::new(403),
    );
    env.page(&format!("{BASE}/5/"), "<p>sem título</p>");
    env.profile();
    env.ok(&["login"]);
    let out = env.ok(&["atualizar", "--podar"]);
    assert!(out.contains("aviso: falharam as entradas [5]"));
    assert!(out.contains("o SUAP negou a listagem das áreas [\"rh\"]"));
    assert!(env.ok(&["status"]).contains("Última atualização: nunca"));
}

#[test]
fn aborts_when_the_session_is_lost() {
    let env = Env::new();
    env.page("/accounts/login/", LOGIN_FORM);
    env.mount(
        "POST",
        "/accounts/login/",
        ResponseTemplate::new(302).insert_header("location", "/"),
    );
    env.page("/", "inicio");
    env.mount(
        "GET",
        &format!("{BASE}/"),
        ResponseTemplate::new(302).insert_header("location", "/accounts/login/"),
    );
    env.profile();
    env.ok(&["login"]);
    assert!(env
        .fails(&["atualizar"])
        .contains("sessão do SUAP ausente ou expirada"));
}

#[test]
fn prints_scheduling_snippets() {
    let env = Env::new();
    for system in ["windows", "cron", "systemd"] {
        assert!(env
            .ok(&["agendar", "--sistema", system])
            .contains("atualizar --se-vencido --quiet"));
    }
    assert!(env.ok(&["agendar"]).contains("--profile default"));
    assert!(env
        .fails(&["agendar", "--sistema", "launchd"])
        .contains("sistema inválido"));
}
