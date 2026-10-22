//! Configurações da atualização e a atualização em si, com progresso e cancelamento.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock, Mutex, PoisonError,
};

use baseconhecimento_core::{
    clock,
    crawl::{self, Report, UpdateOptions},
    index,
    settings::Settings,
    store::Store,
    Visibility,
};

use crate::{convert, failed, Base, BaseError};

/// Só uma atualização por vez no processo (a da tela e a agendada não se atropelam).
static UPDATING: AtomicBool = AtomicBool::new(false);
static CANCEL: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));
/// O andamento da rodada atual (ou da última), para a tela consultar de tempos em tempos.
static PROGRESS: Mutex<Progress> = Mutex::new(Progress::new());

/// Quantas linhas recentes o andamento guarda.
const RECENT: usize = 20;

struct Progress {
    listed: Option<u32>,
    done: u32,
    recent: Vec<String>,
}

impl Progress {
    const fn new() -> Self {
        Self {
            listed: None,
            done: 0,
            recent: Vec::new(),
        }
    }

    /// Anota uma linha do `crawl`: "N entrada(s) para conferir" dá o total; "entrada ID: ..." conta uma.
    fn note(&mut self, line: &str) {
        if let Some(total) = line.strip_suffix(" entrada(s) para conferir") {
            self.listed = total.parse().ok();
        }
        if line.starts_with("entrada ") {
            self.done += 1;
        }
        if self.recent.len() == RECENT {
            self.recent.remove(0);
        }
        self.recent.push(line.to_owned());
    }
}

fn progress() -> std::sync::MutexGuard<'static, Progress> {
    PROGRESS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Marca a atualização em andamento e a desmarca ao sair, mesmo com erro.
struct Gate;

impl Gate {
    fn enter() -> Result<Self, BaseError> {
        if UPDATING.swap(true, Ordering::SeqCst) {
            return Err(failed("já há uma atualização em andamento"));
        }
        CANCEL.store(false, Ordering::SeqCst);
        *progress() = Progress::new();
        Ok(Self)
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        UPDATING.store(false, Ordering::SeqCst);
    }
}

/// Pede para a atualização em andamento parar antes da próxima entrada (o que veio fica salvo).
#[uniffi::export]
pub fn cancel_update() {
    CANCEL.store(true, Ordering::SeqCst);
}

/// Se há uma atualização em andamento neste processo.
#[uniffi::export]
pub fn update_running() -> bool {
    UPDATING.load(Ordering::SeqCst)
}

/// O andamento da atualização: só ids e contagens, nunca títulos nem textos.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UpdateProgress {
    pub running: bool,
    /// Quantas entradas a rodada vai conferir (conhecido depois de listar as áreas).
    pub listed: Option<u32>,
    /// Quantas já foram conferidas.
    pub done: u32,
    /// As últimas linhas (`entrada 12: nova`), da mais antiga à mais nova.
    pub recent: Vec<String>,
}

/// Como a atualização em andamento (ou a última) está; a tela consulta de tempos em tempos.
#[uniffi::export]
pub fn update_progress() -> UpdateProgress {
    let state = progress();
    UpdateProgress {
        running: update_running(),
        listed: state.listed,
        done: state.done,
        recent: state.recent.clone(),
    }
}

/// O que a pessoa ajusta na atualização (por perfil).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UpdateSettings {
    /// Se a atualização agendada pode rodar sozinha.
    pub auto_update: bool,
    /// Dias entre uma atualização automática e a seguinte (1 a 365).
    pub interval_days: u32,
    /// Se os anexos e as imagens são baixados junto.
    pub attachments: bool,
    /// Segundos Unix da última atualização completa.
    pub last_update: Option<i64>,
    /// Segundos Unix de quando a próxima automática vence.
    pub next_due: Option<i64>,
}

/// Quantas entradas há e quando a próxima atualização vence.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Status {
    pub entries: u32,
    pub public: u32,
    pub private: u32,
    pub confidential: u32,
    pub settings: UpdateSettings,
    /// A atualização automática está ligada e o intervalo já venceu.
    pub due: bool,
}

/// Opções de uma rodada.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct UpdateParams {
    /// Baixa de novo até o que não mudou.
    pub force: bool,
    /// Apaga localmente o que sumiu da listagem do SUAP.
    pub prune: bool,
    /// Só estes ids (vazio: todas as áreas).
    pub ids: Vec<u64>,
    /// Baixar anexos nesta rodada; sem valor, vale a configuração.
    pub with_attachments: Option<bool>,
}

/// O resultado da rodada.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UpdateSummary {
    pub areas: u32,
    pub listed: u32,
    pub added: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub removed: u32,
    pub attachments: u32,
    pub attachment_failures: u32,
    pub images: u32,
    pub image_failures: u32,
    /// Ids que a pessoa logada não enxerga (403/404).
    pub inaccessible: Vec<u64>,
    /// Ids que falharam por outro motivo.
    pub failed: Vec<u64>,
    /// Áreas cuja listagem o SUAP negou.
    pub areas_denied: Vec<String>,
    /// A rodada foi interrompida; o resultado é parcial.
    pub cancelled: bool,
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

impl From<Report> for UpdateSummary {
    fn from(report: Report) -> Self {
        Self {
            areas: count(report.areas),
            listed: count(report.listed),
            added: count(report.added),
            updated: count(report.updated),
            unchanged: count(report.unchanged),
            removed: count(report.removed),
            attachments: count(report.attachments),
            attachment_failures: count(report.attachment_failures),
            images: count(report.images),
            image_failures: count(report.image_failures),
            inaccessible: report.inaccessible,
            failed: report.failed,
            areas_denied: report.areas_denied,
            cancelled: report.cancelled,
        }
    }
}

fn view(settings: &Settings) -> UpdateSettings {
    UpdateSettings {
        auto_update: settings.auto_update,
        interval_days: settings.interval_days,
        attachments: settings.attachments,
        last_update: settings.last_update,
        next_due: settings.next_due(),
    }
}

#[uniffi::export]
impl Base {
    /// As configurações da atualização (os padrões se ainda não foram ajustadas).
    pub fn settings(&self) -> Result<UpdateSettings, BaseError> {
        Ok(view(&Settings::load(&self.root).map_err(convert)?))
    }

    /// Grava as configurações, mantendo a data da última atualização; o intervalo é validado.
    pub fn save_settings(
        &self,
        auto_update: bool,
        interval_days: u32,
        attachments: bool,
    ) -> Result<UpdateSettings, BaseError> {
        let mut settings = Settings::load(&self.root).map_err(convert)?;
        settings.auto_update = auto_update;
        settings.interval_days = interval_days;
        settings.attachments = attachments;
        settings.save(&self.root).map_err(convert)?;
        Ok(view(&settings))
    }

    /// Quantas entradas há, por visibilidade, e se a atualização automática venceu.
    pub fn status(&self) -> Result<Status, BaseError> {
        let store = Store::open(self.root.clone()).map_err(convert)?;
        let manifest = store.manifest().map_err(convert)?;
        let settings = Settings::load(&self.root).map_err(convert)?;
        let of = |wanted: Visibility| {
            count(
                manifest
                    .values()
                    .filter(|r| r.visibility == Some(wanted))
                    .count(),
            )
        };
        Ok(Status {
            entries: count(manifest.len()),
            public: of(Visibility::Public),
            private: of(Visibility::Private),
            confidential: of(Visibility::Confidential),
            due: settings.auto_update && settings.is_due(clock::now()),
            settings: view(&settings),
        })
    }

    /// Refaz o índice de busca a partir dos arquivos locais; devolve quantas entradas indexou.
    pub fn rebuild_index(&self) -> Result<u32, BaseError> {
        let store = Store::open(self.root.clone()).map_err(convert)?;
        let mut search_index = index::open(&store).map_err(convert)?;
        let manifest = store.manifest().map_err(convert)?;
        let total = index::rebuild(&mut search_index, &store, &manifest, clock::now());
        Ok(count(total.map_err(convert)?))
    }

    /// Baixa o que mudou na Base de Conhecimento e atualiza o índice. Bloqueia até terminar: chame
    /// fora da thread principal e acompanhe por `update_progress`.
    pub fn update(&self, params: UpdateParams) -> Result<UpdateSummary, BaseError> {
        self.run_update(params, &mut |line| progress().note(line))
    }

    /// Como `update`, mas só se a atualização automática está ligada e o intervalo venceu (é o que
    /// a tarefa agendada chama); `None` quando não havia nada a fazer.
    pub fn update_if_due(&self) -> Result<Option<UpdateSummary>, BaseError> {
        let settings = Settings::load(&self.root).map_err(convert)?;
        if !settings.auto_update || !settings.is_due(clock::now()) {
            return Ok(None);
        }
        self.update(UpdateParams::default()).map(Some)
    }
}

impl Base {
    fn run_update(
        &self,
        params: UpdateParams,
        progress: &mut dyn FnMut(&str),
    ) -> Result<UpdateSummary, BaseError> {
        let _gate = Gate::enter()?;
        if !self.has_session() {
            return Err(BaseError::SessionExpired);
        }
        let mut settings = Settings::load(&self.root).map_err(convert)?;
        let store = Store::open(self.root.clone()).map_err(convert)?;
        let mut search_index = index::open(&store).map_err(convert)?;
        let options = UpdateOptions {
            force: params.force,
            prune: params.prune,
            attachments: params.with_attachments.unwrap_or(settings.attachments),
            only: Some(params.ids).filter(|ids| !ids.is_empty()),
            scan: None,
            cancel: Some(CANCEL.clone()),
        };
        let report = crawl::update(
            &self.source()?,
            &store,
            &mut search_index,
            &options,
            self.config.base_url.as_str(),
            progress,
        )
        .map_err(convert)?;
        if options.only.is_none() && report.areas_denied.is_empty() && !report.cancelled {
            settings.last_update = Some(clock::now());
            settings.save(&self.root).map_err(convert)?;
        }
        Ok(report.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{serial, Env};

    fn logged_in() -> (Env, Arc<Base>) {
        let env = Env::new();
        env.suap();
        let base = env.base();
        base.import_session_cookies("sessionid=abc".into()).unwrap();
        (env, base)
    }

    #[test]
    fn settings_default_save_and_validate() {
        let (_env, base) = logged_in();
        let defaults = base.settings().unwrap();
        assert!(defaults.auto_update && defaults.attachments);
        assert_eq!((defaults.interval_days, defaults.last_update), (7, None));
        assert_eq!(defaults.next_due, None);
        let saved = base.save_settings(false, 3, false).unwrap();
        assert_eq!(base.settings().unwrap(), saved);
        assert!(!saved.auto_update && !saved.attachments && saved.interval_days == 3);
        assert!(base.save_settings(true, 0, true).is_err());
        assert!(base.save_settings(true, 366, true).is_err());
        std::fs::write(base.root.join("settings.json"), "quebrado").unwrap();
        assert!(base.settings().is_err());
        assert!(base.save_settings(true, 7, true).is_err());
        assert!(base.status().is_err());
    }

    #[test]
    fn updates_reports_progress_and_remembers_when() {
        let _turn = serial();
        let (_env, base) = logged_in();
        let summary = base.update(UpdateParams::default()).unwrap();
        assert_eq!((summary.areas, summary.listed, summary.added), (1, 3, 2));
        assert_eq!(
            (summary.inaccessible.clone(), summary.attachments),
            (vec![3], 1)
        );
        assert_eq!(summary.images, 1);
        assert!(!summary.cancelled && summary.failed.is_empty());
        let now = update_progress();
        let seen = now.recent.join("\n");
        assert!(!now.running && now.listed == Some(3) && now.done == 3);
        assert!(seen.contains("entrada 1: nova") && seen.contains("entrada 3: sem acesso"));
        assert!(!seen.contains("toner"), "o progresso não mostra títulos");

        let status = base.status().unwrap();
        assert_eq!(
            (
                status.entries,
                status.public,
                status.private,
                status.confidential
            ),
            (2, 1, 1, 0)
        );
        assert!(status.settings.last_update.is_some() && !status.due);
        assert!(status.settings.next_due.is_some());

        // De novo: nada mudou; sem ouvinte; e sem anexos nesta rodada.
        let again = base.update(UpdateParams::default()).unwrap();
        assert_eq!((again.added, again.updated, again.unchanged), (0, 0, 2));
        let only = UpdateParams {
            force: true,
            ids: vec![2],
            with_attachments: Some(false),
            ..UpdateParams::default()
        };
        let one = base.update(only).unwrap();
        assert_eq!((one.updated, one.listed, one.attachments), (1, 1, 0));
        let pruned = base
            .update(UpdateParams {
                prune: true,
                ..UpdateParams::default()
            })
            .unwrap();
        assert_eq!(pruned.removed, 0);

        assert_eq!(base.rebuild_index().unwrap(), 2);
    }

    #[test]
    fn the_scheduled_round_only_runs_when_due() {
        let _turn = serial();
        let (_env, base) = logged_in();
        base.save_settings(false, 7, true).unwrap();
        assert!(base.update_if_due().unwrap().is_none());
        base.save_settings(true, 7, true).unwrap();
        let first = base.update_if_due().unwrap().unwrap();
        assert_eq!(first.added, 2);
        assert!(base.update_if_due().unwrap().is_none());
        // Intervalo vencido (a última foi há 8 dias): roda de novo.
        let mut settings = Settings::load(&base.root).unwrap();
        settings.last_update = Some(clock::now() - 8 * 86_400);
        settings.save(&base.root).unwrap();
        assert!(base.status().unwrap().due);
        assert!(base.update_if_due().unwrap().is_some());
    }

    #[test]
    fn needs_a_session_and_only_one_round_at_a_time() {
        let _turn = serial();
        let env = Env::new();
        env.suap();
        let base = env.base();
        assert!(matches!(
            base.update(UpdateParams::default()),
            Err(BaseError::SessionExpired)
        ));
        assert!(!update_running());
        let gate = Gate::enter().unwrap();
        assert!(update_running());
        let busy = base.update(UpdateParams::default()).unwrap_err();
        assert!(busy.to_string().contains("em andamento"));
        drop(gate);
        assert!(!update_running());
    }

    #[test]
    fn a_cancelled_round_is_partial_and_does_not_count_as_done() {
        let _turn = serial();
        let (_env, base) = logged_in();
        // Pede o cancelamento assim que a primeira linha de progresso chega.
        let summary = base
            .run_update(UpdateParams::default(), &mut |_| cancel_update())
            .unwrap();
        assert!(summary.cancelled);
        assert_eq!(base.settings().unwrap().last_update, None);
    }

    #[test]
    fn progress_keeps_the_last_lines_and_ignores_a_bad_total() {
        let mut state = Progress::new();
        state.note("muitas entrada(s) para conferir");
        assert_eq!(state.listed, None);
        for id in 0..=RECENT {
            state.note(&format!("entrada {id}: nova"));
        }
        assert_eq!(state.done as usize, RECENT + 1);
        assert_eq!(state.recent.len(), RECENT);
        assert_eq!(state.recent[0], "entrada 1: nova");
    }

    #[test]
    fn counts_saturate_and_a_broken_copy_surfaces() {
        assert_eq!(count(usize::MAX), u32::MAX);
        let (_env, base) = logged_in();
        let _turn = serial();
        // Um manifesto ilegível: o erro chega ao app e o portão é liberado.
        std::fs::create_dir_all(&base.root).unwrap();
        std::fs::write(base.root.join("manifesto.json"), "quebrado").unwrap();
        assert!(base.update(UpdateParams::default()).is_err());
        assert!(!update_running());
        assert!(base.rebuild_index().is_err());
    }
}
