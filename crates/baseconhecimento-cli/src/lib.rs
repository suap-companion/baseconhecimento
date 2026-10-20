//! Lógica da linha de comando do `baseconhecimento`.
//!
//! O ponto de entrada (`main.rs`) é um invólucro fino; tudo o que se testa vive aqui.

use std::{error::Error, ffi::OsString, fs, io::Write, path::PathBuf};

use baseconhecimento_core::{
    clock,
    crawl::{self, UpdateOptions},
    index,
    schedule::{self, Scheduler},
    settings::Settings,
    source::SuapSource,
    store::{Record, Store},
    Visibility,
};
use clap::{Args, Parser, Subcommand};
use suap_core::{
    list_profiles, load_config, remove_config, save_config, AppPaths, SuapConfig, DEFAULT_PROFILE,
};

/// Variável de ambiente com a senha do SUAP (a mesma do `chamados`).
pub const PASSWORD_ENV: &str = "SUAP_PASSWORD";

#[derive(Debug, Parser)]
#[command(
    name = "baseconhecimento",
    version,
    about = "Base de Conhecimento do SUAP, offline e pesquisável"
)]
struct Cli {
    /// Perfil (ambiente) a usar; cada perfil tem sua configuração, sessão e cópia local.
    #[arg(long, global = true, default_value = DEFAULT_PROFILE)]
    profile: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Mostra onde ficam a configuração, a sessão e a cópia local do perfil.
    Paths,
    /// Gerencia perfis (ambientes SUAP).
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Entra no SUAP (senha na variável SUAP_PASSWORD) e salva a sessão do perfil.
    Login {
        /// Usuário; por padrão, o do perfil.
        #[arg(long)]
        username: Option<String>,
    },
    /// Verifica se a sessão salva ainda vale.
    SessionStatus,
    /// Baixa o que mudou na Base de Conhecimento e atualiza o índice de busca.
    Atualizar(UpdateArgs),
    /// Lista as entradas baixadas.
    Listar {
        /// Só esta área (parte do nome, sem diferenciar maiúsculas).
        #[arg(long)]
        area: Option<String>,
        /// Só esta visibilidade: publica, privada ou sigilosa.
        #[arg(long)]
        visibilidade: Option<String>,
        /// Imprime JSON.
        #[arg(long)]
        json: bool,
    },
    /// Mostra uma entrada baixada, em Markdown.
    Mostrar {
        id: u64,
        /// Imprime os metadados em JSON, com o caminho do arquivo.
        #[arg(long)]
        json: bool,
    },
    /// Busca por palavras nas entradas e nos anexos (TXT, MD e PDF) baixados.
    Buscar {
        /// Palavras da busca.
        #[arg(required = true)]
        consulta: Vec<String>,
        /// Quantos trechos mostrar.
        #[arg(long, default_value_t = 5)]
        limite: usize,
        /// Imprime JSON.
        #[arg(long)]
        json: bool,
    },
    /// Refaz o índice de busca a partir dos arquivos locais.
    Reindexar,
    /// Resume a cópia local e quando a próxima atualização vence.
    Status {
        /// Imprime JSON.
        #[arg(long)]
        json: bool,
    },
    /// Mostra ou altera as configurações da atualização do perfil.
    Config {
        /// Liga (true) ou desliga (false) a atualização automática (`atualizar --se-vencido`).
        #[arg(long)]
        auto_atualizar: Option<bool>,
        /// Dias entre atualizações automáticas (1 a 365; o padrão é 7).
        #[arg(long)]
        intervalo_dias: Option<u32>,
        /// Baixa (true) ou não (false) os anexos.
        #[arg(long)]
        anexos: Option<bool>,
    },
    /// Imprime o comando que agenda a atualização semanal no seu sistema.
    Agendar {
        /// windows, cron ou systemd (por padrão, o do sistema atual).
        #[arg(long)]
        sistema: Option<String>,
    },
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Baixa de novo até o que não mudou.
    #[arg(long)]
    forcar: bool,
    /// Só atualiza se a atualização automática estiver ligada e o intervalo tiver vencido (para o agendador).
    #[arg(long)]
    se_vencido: bool,
    /// Apaga da cópia local o que sumiu da listagem do SUAP.
    #[arg(long)]
    podar: bool,
    /// Não baixa os anexos nesta rodada.
    #[arg(long)]
    sem_anexos: bool,
    /// Só mostra problemas (para o agendador).
    #[arg(long)]
    quiet: bool,
    /// Procura também entradas que nenhuma área lista (buracos nos ids e ids acima do maior).
    #[arg(long)]
    varrer: bool,
    /// Com --varrer: para depois de tantos ids seguidos sem entrada visível.
    #[arg(long, default_value_t = 50)]
    parada: usize,
    /// Só estes ids de entradas, sem varrer as áreas.
    #[arg(long = "id")]
    ids: Vec<u64>,
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    /// Cria um perfil (sem nome, usa o de --profile).
    Init(ProfileArgs),
    /// Altera campos de um perfil existente.
    Update(ProfileArgs),
    /// Apaga um perfil e a sessão dele.
    Remove {
        name: String,
        /// Confirma a remoção.
        #[arg(long)]
        yes: bool,
        /// Apaga também a cópia local da Base de Conhecimento do perfil.
        #[arg(long)]
        apagar_dados: bool,
    },
    /// Exibe a configuração de um perfil, sem dados sensíveis.
    Show {
        name: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Lista os perfis.
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
struct ProfileArgs {
    /// Nome do perfil; por padrão, o de --profile.
    name: Option<String>,
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long)]
    username: Option<String>,
}

const OVERDUE: &str = "Atualização vencida: execute `baseconhecimento atualizar`.";

type Outcome = Result<(), Box<dyn Error>>;

/// Escreve uma linha. Um `writeln!(...)?;` quebrado em várias linhas deixa o `)?;` sem cobertura.
fn say(out: &mut dyn Write, text: &str) -> Outcome {
    Ok(writeln!(out, "{text}")?)
}

/// Executa a CLI com `args`, escrevendo em `out`/`err`, e devolve o código de saída.
///
/// `password` é o valor de [`PASSWORD_ENV`]; vem de fora para os testes poderem injetá-lo.
pub fn run<I, T>(
    args: I,
    paths: &AppPaths,
    password: Option<String>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let code = error.exit_code();
            let _ = if error.use_stderr() {
                write!(err, "{error}")
            } else {
                write!(out, "{error}")
            };
            return code;
        }
    };
    let result = paths
        .clone()
        .with_profile(&cli.profile)
        .map_err(Into::into)
        .and_then(|paths| dispatch(cli.command, &paths, password, out));
    match result {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(err, "erro: {error}");
            1
        }
    }
}

fn dispatch(
    command: Command,
    paths: &AppPaths,
    password: Option<String>,
    out: &mut dyn Write,
) -> Outcome {
    match command {
        Command::Paths => show_paths(paths, out),
        Command::Profile { command } => profile(command, paths, out),
        Command::Login { username } => login(paths, username, password, out),
        Command::SessionStatus => session_status(paths, out),
        Command::Atualizar(args) => atualizar(paths, args, out),
        Command::Listar {
            area,
            visibilidade,
            json,
        } => listar(paths, area.as_deref(), visibilidade.as_deref(), json, out),
        Command::Mostrar { id, json } => mostrar(paths, id, json, out),
        Command::Buscar {
            consulta,
            limite,
            json,
        } => buscar(paths, &consulta.join(" "), limite, json, out),
        Command::Reindexar => reindexar(paths, out),
        Command::Status { json } => status(paths, json, out),
        Command::Config {
            auto_atualizar,
            intervalo_dias,
            anexos,
        } => config(paths, auto_atualizar, intervalo_dias, anexos, out),
        Command::Agendar { sistema } => agendar(paths, sistema.as_deref(), out),
    }
}

/// Pasta da cópia local do perfil.
pub fn data_root(paths: &AppPaths) -> PathBuf {
    paths.data_dir().join("base").join(paths.profile())
}

fn profile_config(paths: &AppPaths) -> Result<SuapConfig, Box<dyn Error>> {
    let profile = paths.profile();
    load_config(paths)?.ok_or_else(|| {
        format!("perfil {profile:?} não configurado: execute `baseconhecimento profile init`")
            .into()
    })
}

fn show_paths(paths: &AppPaths, out: &mut dyn Write) -> Outcome {
    writeln!(out, "perfil:      {}", paths.profile())?;
    writeln!(out, "configuração: {}", paths.config_file().display())?;
    writeln!(out, "sessão:      {}", paths.session_file().display())?;
    writeln!(out, "cópia local: {}", data_root(paths).display())?;
    Ok(())
}

fn profile_paths(paths: &AppPaths, name: Option<&str>) -> Result<AppPaths, Box<dyn Error>> {
    match name {
        Some(name) => Ok(paths.clone().with_profile(name)?),
        None => Ok(paths.clone()),
    }
}

fn apply_profile_args(config: &mut SuapConfig, args: ProfileArgs) -> Outcome {
    let base_url = args.base_url.map(|text| text.parse()).transpose()?;
    if let Some(base_url) = base_url {
        config.base_url = base_url;
    }
    if args.username.is_some() {
        config.username = args.username;
    }
    config.updated_at = u64::try_from(clock::now())? * 1000;
    Ok(())
}

fn profile(command: ProfileCommand, paths: &AppPaths, out: &mut dyn Write) -> Outcome {
    match command {
        ProfileCommand::Init(args) => {
            let paths = profile_paths(paths, args.name.as_deref())?;
            let name = paths.profile().to_owned();
            if load_config(&paths)?.is_some() {
                return Err(format!(
                    "o perfil {name:?} já existe: altere-o com `baseconhecimento profile update {name}`"
                )
                .into());
            }
            let mut config = SuapConfig::default();
            apply_profile_args(&mut config, args)?;
            save_config(&paths, &config)?;
            let text = format!("Perfil {name} criado em {}", paths.config_file().display());
            say(out, &text)?;
        }
        ProfileCommand::Update(args) => {
            let paths = profile_paths(paths, args.name.as_deref())?;
            let name = paths.profile().to_owned();
            let mut config = profile_config(&paths)?;
            if args.base_url.is_none() && args.username.is_none() {
                return Err("nada a alterar: informe --base-url ou --username".into());
            }
            apply_profile_args(&mut config, args)?;
            save_config(&paths, &config)?;
            let text = format!(
                "Perfil {name} atualizado em {}",
                paths.config_file().display()
            );
            say(out, &text)?;
        }
        ProfileCommand::Remove {
            name,
            yes,
            apagar_dados,
        } => {
            let paths = profile_paths(paths, Some(&name))?;
            profile_config(&paths)?;
            if !yes {
                return Err(format!(
                    "a remoção apaga a configuração e a sessão do perfil {name:?}: confirme com --yes"
                )
                .into());
            }
            remove_config(&paths)?;
            if paths.session_file().exists() {
                fs::remove_file(paths.session_file())?;
            }
            if apagar_dados && data_root(&paths).exists() {
                fs::remove_dir_all(data_root(&paths))?;
            }
            writeln!(out, "Perfil {name} removido.")?;
        }
        ProfileCommand::Show { name, json } => {
            let paths = profile_paths(paths, name.as_deref())?;
            let config = profile_config(&paths)?;
            let (name, url) = (paths.profile(), config.base_url.as_str());
            let user = config.username.as_deref().unwrap_or("");
            if json {
                let value = serde_json::json!({ "perfil": name, "base_url": url, "usuario": user });
                writeln!(out, "{value}")?;
            } else {
                writeln!(out, "perfil:  {name}\nbase:    {url}\nusuário: {user}")?;
            }
        }
        ProfileCommand::List { json } => {
            let profiles = list_profiles(paths)?;
            if json {
                let items: Vec<_> = profiles
                    .iter()
                    .map(|(name, config)| {
                        serde_json::json!({ "perfil": name, "base_url": config.base_url.as_str(), "usuario": config.username })
                    })
                    .collect();
                writeln!(out, "{}", serde_json::Value::Array(items))?;
            } else {
                for (name, config) in profiles {
                    let user = config.username.unwrap_or_default();
                    writeln!(out, "{name}\t{}\t{user}", config.base_url)?;
                }
            }
        }
    }
    Ok(())
}

fn login(
    paths: &AppPaths,
    username: Option<String>,
    password: Option<String>,
    out: &mut dyn Write,
) -> Outcome {
    let config = profile_config(paths)?;
    let username = username
        .or_else(|| config.username.clone())
        .ok_or("usuário não informado: use --username ou `profile update --username`")?;
    let password = password.ok_or_else(|| {
        format!("senha não informada: defina a variável de ambiente {PASSWORD_ENV}")
    })?;
    SuapSource::open(paths, &config)?.login(&username, &password)?;
    let text = format!(
        "Login realizado como {username} (perfil {}). Sessão salva em {}",
        paths.profile(),
        paths.session_file().display()
    );
    say(out, &text)?;
    Ok(())
}

fn session_status(paths: &AppPaths, out: &mut dyn Write) -> Outcome {
    let config = profile_config(paths)?;
    let profile = paths.profile();
    if !paths.session_file().exists() {
        return Err(format!(
            "nenhuma sessão salva para o perfil {profile:?}: execute `baseconhecimento login`"
        )
        .into());
    }
    if !SuapSource::open(paths, &config)?.is_authenticated()? {
        return Err(format!(
            "a sessão do perfil {profile:?} expirou: execute `baseconhecimento login`"
        )
        .into());
    }
    let text = format!("Sessão válida (perfil {profile}, {}).", config.base_url);
    say(out, &text)?;
    Ok(())
}

fn atualizar(paths: &AppPaths, args: UpdateArgs, out: &mut dyn Write) -> Outcome {
    let config = profile_config(paths)?;
    let root = data_root(paths);
    let mut settings = Settings::load(&root)?;
    if args.se_vencido {
        if !settings.auto_update {
            writeln!(out, "Atualização automática desligada: nada a fazer.")?;
            return Ok(());
        }
        if !settings.is_due(clock::now()) {
            let next = settings.next_due().map(clock::iso).unwrap_or_default();
            let text = format!("Ainda não venceu (próxima a partir de {next}): nada a fazer.");
            say(out, &text)?;
            return Ok(());
        }
    }
    if !paths.session_file().exists() {
        return Err(format!(
            "nenhuma sessão salva para o perfil {:?}: execute `baseconhecimento login`",
            paths.profile()
        )
        .into());
    }
    let source = SuapSource::open(paths, &config)?;
    let store = Store::open(root.clone())?;
    let mut search_index = index::open(&store)?;
    let options = UpdateOptions {
        force: args.forcar,
        prune: args.podar,
        attachments: settings.attachments && !args.sem_anexos,
        scan: args.varrer.then_some(args.parada),
        only: Some(args.ids.clone()).filter(|ids| !ids.is_empty()),
    };
    let quiet = args.quiet;
    let mut progress = |line: &str| {
        if !quiet {
            let _ = writeln!(std::io::stderr(), "{line}");
        }
    };
    let report = crawl::update(
        &source,
        &store,
        &mut search_index,
        &options,
        config.base_url.as_str(),
        &mut progress,
    )?;
    if options.only.is_none() && report.areas_denied.is_empty() {
        settings.last_update = Some(clock::now());
        settings.save(&root)?;
    }
    if !quiet {
        let text = format!(
            "Concluído: {} nova(s), {} atualizada(s), {} sem mudança, {} sem acesso, {} falha(s); anexos: {} baixado(s), {} falha(s); {} removida(s).",
            report.added,
            report.updated,
            report.unchanged,
            report.inaccessible.len(),
            report.failed.len(),
            report.attachments,
            report.attachment_failures,
            report.removed
        );
        say(out, &text)?;
    }
    if !report.failed.is_empty() {
        writeln!(out, "aviso: falharam as entradas {:?}", report.failed)?;
    }
    if !report.areas_denied.is_empty() {
        let text = format!(
            "aviso: o SUAP negou a listagem das áreas {:?}; a próxima atualização completa tenta de novo",
            report.areas_denied
        );
        say(out, &text)?;
    }
    Ok(())
}

fn open_store(paths: &AppPaths) -> Result<Store, Box<dyn Error>> {
    profile_config(paths)?;
    Ok(Store::open(data_root(paths))?)
}

fn listar(
    paths: &AppPaths,
    area: Option<&str>,
    visibilidade: Option<&str>,
    json: bool,
    out: &mut dyn Write,
) -> Outcome {
    let wanted = visibilidade
        .map(|name| {
            Visibility::parse(name).ok_or("visibilidade inválida: use publica, privada ou sigilosa")
        })
        .transpose()?;
    let area = area.map(str::to_lowercase);
    let manifest = open_store(paths)?.manifest()?;
    let rows: Vec<(&u64, &Record)> = manifest
        .iter()
        .filter(|(_, record)| wanted.is_none() || record.visibility == wanted)
        .filter(|(_, record)| {
            area.as_ref().is_none_or(|needle| {
                record
                    .area
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(needle)
            })
        })
        .collect();
    if json {
        let items: Vec<_> = rows
            .iter()
            .map(|(id, record)| entry_json(**id, record))
            .collect();
        writeln!(out, "{}", serde_json::Value::Array(items))?;
    } else {
        for (id, record) in rows {
            let visibility = record.visibility.map_or("-", Visibility::as_str);
            let area = record.area.as_deref().unwrap_or("-");
            writeln!(out, "{id}\t{visibility}\t{area}\t{}", record.title)?;
        }
    }
    Ok(())
}

fn entry_json(id: u64, record: &Record) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "titulo": record.title,
        "area": record.area,
        "visibilidade": record.visibility.map(Visibility::as_str),
        "tags": record.tags,
        "atualizado_em": record.updated_at,
        "baixado_em": clock::iso(record.fetched_at),
        "anexos": record.attachments,
    })
}

fn mostrar(paths: &AppPaths, id: u64, json: bool, out: &mut dyn Write) -> Outcome {
    let store = open_store(paths)?;
    let file = store.entry_file(id)?.ok_or_else(|| {
        format!("entrada {id} não está na cópia local: execute `baseconhecimento atualizar`")
    })?;
    if json {
        let mut value = entry_json(id, &store.manifest()?[&id]);
        value["arquivo"] = serde_json::Value::String(file.display().to_string());
        writeln!(out, "{value}")?;
    } else {
        write!(out, "{}", fs::read_to_string(file)?)?;
    }
    Ok(())
}

fn buscar(paths: &AppPaths, query: &str, limit: usize, json: bool, out: &mut dyn Write) -> Outcome {
    let store = open_store(paths)?;
    let found = index::search(&index::open(&store)?, &store.manifest()?, query, limit)?;
    if json {
        let items: Vec<_> = found
            .iter()
            .map(|hit| {
                serde_json::json!({
                    "id": hit.id, "titulo": hit.title, "area": hit.area,
                    "visibilidade": hit.visibility.map(Visibility::as_str),
                    "origem": hit.source, "pagina": hit.page, "trecho": hit.text, "pontuacao": hit.score,
                })
            })
            .collect();
        writeln!(out, "{}", serde_json::Value::Array(items))?;
        return Ok(());
    }
    if found.is_empty() {
        writeln!(out, "Nada encontrado.")?;
    }
    for hit in found {
        let visibility = hit.visibility.map_or("-", Visibility::as_str);
        let snippet = hit.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = format!(
            "[{}] {} ({visibility}; {})\n    {snippet}",
            hit.id, hit.title, hit.source
        );
        say(out, &text)?;
    }
    Ok(())
}

fn reindexar(paths: &AppPaths, out: &mut dyn Write) -> Outcome {
    let store = open_store(paths)?;
    let mut search_index = index::open(&store)?;
    let total = index::rebuild(&mut search_index, &store, &store.manifest()?, clock::now())?;
    writeln!(out, "Índice refeito: {total} entrada(s).")?;
    Ok(())
}

fn status(paths: &AppPaths, json: bool, out: &mut dyn Write) -> Outcome {
    let store = open_store(paths)?;
    let manifest = store.manifest()?;
    let settings = Settings::load(store.root())?;
    let count = |wanted: Visibility| {
        manifest
            .values()
            .filter(|r| r.visibility == Some(wanted))
            .count()
    };
    let (public, private, confidential) = (
        count(Visibility::Public),
        count(Visibility::Private),
        count(Visibility::Confidential),
    );
    let last = settings.last_update.map(clock::iso);
    let next = settings.next_due().map(clock::iso);
    if json {
        let value = serde_json::json!({
            "entradas": manifest.len(), "publicas": public, "privadas": private, "sigilosas": confidential,
            "atualizacao_automatica": settings.auto_update, "intervalo_dias": settings.interval_days,
            "ultima_atualizacao": last, "proxima_atualizacao": next,
            "vencida": settings.is_due(clock::now()),
        });
        writeln!(out, "{value}")?;
    } else {
        let total = manifest.len();
        let text = format!(
            "Entradas: {total} ({public} públicas, {private} privadas, {confidential} sigilosas)"
        );
        say(out, &text)?;
        let text = format!("Última atualização: {}", last.as_deref().unwrap_or("nunca"));
        say(out, &text)?;
        let auto = if settings.auto_update {
            "ligada"
        } else {
            "desligada"
        };
        let days = settings.interval_days;
        let text = format!("Atualização automática: {auto} (a cada {days} dia(s))");
        say(out, &text)?;
        if settings.auto_update && settings.is_due(clock::now()) {
            say(out, OVERDUE)?;
        }
    }
    Ok(())
}

fn config(
    paths: &AppPaths,
    auto: Option<bool>,
    days: Option<u32>,
    attachments: Option<bool>,
    out: &mut dyn Write,
) -> Outcome {
    profile_config(paths)?;
    let root = data_root(paths);
    let mut settings = Settings::load(&root)?;
    if auto.is_some() || days.is_some() || attachments.is_some() {
        settings.auto_update = auto.unwrap_or(settings.auto_update);
        settings.interval_days = days.unwrap_or(settings.interval_days);
        settings.attachments = attachments.unwrap_or(settings.attachments);
        settings.save(&root)?;
    }
    writeln!(out, "atualização automática: {}", settings.auto_update)?;
    writeln!(out, "intervalo (dias):       {}", settings.interval_days)?;
    writeln!(out, "baixar anexos:          {}", settings.attachments)?;
    Ok(())
}

fn agendar(paths: &AppPaths, system: Option<&str>, out: &mut dyn Write) -> Outcome {
    let scheduler = match system {
        Some(name) => {
            Scheduler::parse(name).ok_or("sistema inválido: use windows, cron ou systemd")?
        }
        None => Scheduler::detect(),
    };
    let exe = std::env::current_exe()?;
    let text = schedule::snippet(scheduler, &exe.to_string_lossy(), paths.profile())?;
    Ok(out.write_all(text.as_bytes())?)
}
