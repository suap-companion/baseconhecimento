//! A atualização incremental: descobre as entradas, baixa só o que mudou e reindexa.
//!
//! O que a pessoa logada não enxerga (403/404) é pulado e contado; o que ela enxerga, inclusive
//! entradas privadas e sigilosas, é baixado só para a pasta local. O progresso mostra ids e
//! números, nunca títulos nem textos.

use crate::{
    clock,
    index::{reindex_entry, remove_entry},
    parse::{self, ParseError},
    sanitize,
    source::{FetchError, Fetcher},
    store::{render_markdown, Manifest, Record, Store},
    Error,
};
use suap_rag::Index;

/// Opções de uma rodada de atualização.
#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    /// Baixa de novo mesmo as entradas sem mudança.
    pub force: bool,
    /// Apaga localmente o que sumiu da listagem do SUAP.
    pub prune: bool,
    /// Baixa os anexos.
    pub attachments: bool,
    /// Só estes ids (sem varrer as áreas).
    pub only: Option<Vec<u64>>,
}

/// O resultado da rodada.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub areas: usize,
    pub listed: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub attachments: usize,
    /// Anexos que não puderam ser baixados.
    pub attachment_failures: usize,
    /// Ids que a pessoa logada não enxerga (403/404).
    pub inaccessible: Vec<u64>,
    /// Ids que falharam por outro motivo.
    pub failed: Vec<u64>,
    /// Áreas cuja listagem o SUAP negou.
    pub areas_denied: Vec<String>,
}

/// Os ids de todas as áreas visíveis.
fn discover(fetcher: &dyn Fetcher, report: &mut Report) -> Result<Vec<u64>, Error> {
    let areas = parse::parse_areas(&fetcher.page(parse::AREAS_PATH)?);
    report.areas = areas.len();
    let mut ids: Vec<u64> = Vec::new();
    for area in areas {
        match fetcher.page(&parse::dashboard_path(&area.slug)) {
            Ok(html) => {
                for id in parse::parse_entry_ids(&html) {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
            Err(FetchError::NotAuthenticated) => return Err(FetchError::NotAuthenticated.into()),
            Err(_) => report.areas_denied.push(area.slug),
        }
    }
    Ok(ids)
}

/// Baixa os anexos da entrada; devolve os nomes locais.
fn download_attachments(
    fetcher: &dyn Fetcher,
    store: &Store,
    entry: &crate::Entry,
    report: &mut Report,
) -> Result<Vec<String>, Error> {
    store.clear_attachments(entry.id)?;
    let mut names: Vec<String> = Vec::new();
    for attachment in &entry.attachments {
        match fetcher.file(&attachment.url) {
            Ok(bytes) => {
                let name = sanitize::unique(&attachment.name, &names);
                store.save_attachment(entry.id, &name, &bytes)?;
                names.push(name);
                report.attachments += 1;
            }
            Err(FetchError::NotAuthenticated) => return Err(FetchError::NotAuthenticated.into()),
            Err(_) => report.attachment_failures += 1,
        }
    }
    Ok(names)
}

/// O que aconteceu com um id.
enum Step {
    Added,
    Updated,
    Unchanged,
    Inaccessible,
    Failed,
}

#[allow(clippy::too_many_arguments)]
fn sync_entry(
    fetcher: &dyn Fetcher,
    store: &Store,
    index: &mut Index,
    manifest: &mut Manifest,
    options: &UpdateOptions,
    base_url: &str,
    id: u64,
    report: &mut Report,
) -> Result<Step, Error> {
    let html = match fetcher.page(&parse::entry_path(id)) {
        Ok(html) => html,
        Err(FetchError::NotAuthenticated) => return Err(FetchError::NotAuthenticated.into()),
        Err(FetchError::Inaccessible) => return Ok(Step::Inaccessible),
        Err(FetchError::Other(_)) => return Ok(Step::Failed),
    };
    let entry = match parse::parse_entry(id, &html) {
        Ok(entry) => entry,
        Err(ParseError::NoTitle(_)) => return Ok(Step::Failed),
    };
    let known = manifest.get(&id);
    let file_exists = store.entry_file(id)?.is_some();
    let hash = entry.content_hash();
    let same = known.is_some_and(|record| record.hash == hash);
    if same && file_exists && !options.force {
        return Ok(Step::Unchanged);
    }
    let step = if known.is_some() {
        Step::Updated
    } else {
        Step::Added
    };
    let now = clock::now();
    let names = if options.attachments {
        download_attachments(fetcher, store, &entry, report)?
    } else {
        Vec::new()
    };
    store.save_entry(&entry, &render_markdown(&entry, base_url, now, &names))?;
    reindex_entry(index, store, id, &names, now)?;
    manifest.insert(
        id,
        Record {
            title: entry.title,
            area: entry.area,
            visibility: entry.visibility,
            tags: entry.tags,
            updated_at: entry.updated_at,
            hash,
            fetched_at: now,
            attachments: names,
        },
    );
    store.save_manifest(manifest)?;
    Ok(step)
}

/// Roda uma atualização. `base_url` só entra no `url:` do frontmatter.
pub fn update(
    fetcher: &dyn Fetcher,
    store: &Store,
    index: &mut Index,
    options: &UpdateOptions,
    base_url: &str,
    progress: &mut dyn FnMut(&str),
) -> Result<Report, Error> {
    let mut report = Report::default();
    let mut manifest = store.manifest()?;
    let ids = match &options.only {
        Some(only) => only.clone(),
        None => discover(fetcher, &mut report)?,
    };
    report.listed = ids.len();
    progress(&format!("{} entrada(s) para conferir", ids.len()));
    for id in &ids {
        let step = sync_entry(
            fetcher,
            store,
            index,
            &mut manifest,
            options,
            base_url,
            *id,
            &mut report,
        )?;
        match step {
            Step::Added => report.added += 1,
            Step::Updated => report.updated += 1,
            Step::Unchanged => report.unchanged += 1,
            Step::Inaccessible => report.inaccessible.push(*id),
            Step::Failed => report.failed.push(*id),
        }
        progress(&format!("entrada {id}: {}", label(&step)));
    }
    let complete = options.only.is_none() && report.areas_denied.is_empty();
    if options.prune && complete {
        let gone: Vec<u64> = manifest
            .keys()
            .filter(|id| !ids.contains(id))
            .copied()
            .collect();
        for id in gone {
            store.remove(id)?;
            remove_entry(index, id)?;
            manifest.remove(&id);
            report.removed += 1;
        }
        store.save_manifest(&manifest)?;
    }
    Ok(report)
}

fn label(step: &Step) -> &'static str {
    match step {
        Step::Added => "nova",
        Step::Updated => "atualizada",
        Step::Unchanged => "sem mudança",
        Step::Inaccessible => "sem acesso",
        Step::Failed => "falhou",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::HashMap};

    type Reply<T> = Result<T, FetchError>;

    #[derive(Default)]
    struct Fake {
        pages: RefCell<HashMap<String, Reply<String>>>,
        files: RefCell<HashMap<String, Reply<Vec<u8>>>>,
    }

    impl Fake {
        fn page(&self, path: &str, reply: Reply<String>) {
            self.pages.borrow_mut().insert(path.to_owned(), reply);
        }
        fn file(&self, path: &str, reply: Reply<Vec<u8>>) {
            self.files.borrow_mut().insert(path.to_owned(), reply);
        }
    }

    impl Fetcher for Fake {
        fn page(&self, path: &str) -> Reply<String> {
            self.pages
                .borrow()
                .get(path)
                .cloned()
                .unwrap_or(Err(FetchError::Inaccessible))
        }
        fn file(&self, path: &str) -> Reply<Vec<u8>> {
            self.files
                .borrow()
                .get(path)
                .cloned()
                .unwrap_or(Err(FetchError::Inaccessible))
        }
    }

    fn entry_html(title: &str, updated: &str, body: &str) -> String {
        format!(
            r#"<h2>{title}</h2><span class="status status-privada">Privada</span>
            <div id="resumo"><div class="accordion-body"><p>{body}</p></div></div>
            <div class="tab-pane" data-title="Anexos"><a href="/media/guia.txt">guia.txt</a><a href="/media/guia.txt">guia.txt</a><a href="/media/falha.bin">falha.bin</a></div>
            <ul><li>Última Atualização por Pessoa em {updated}</li></ul>"#
        )
    }

    fn world() -> Fake {
        let fake = Fake::default();
        fake.page(
            "/centralservicos/baseconhecimento/",
            Ok(r#"<a class="box-item" href="/centralservicos/baseconhecimento/dashboard/ti/"><h4>TI</h4></a>
                  <a class="box-item" href="/centralservicos/baseconhecimento/dashboard/rh/"><h4>RH</h4></a>"#.into()),
        );
        fake.page(
            "/centralservicos/baseconhecimento/dashboard/ti/",
            Ok(r#"<a href="/centralservicos/baseconhecimento/1/">a</a><a href="/centralservicos/baseconhecimento/2/">b</a>
                  <a href="/centralservicos/baseconhecimento/3/">c</a><a href="/centralservicos/baseconhecimento/4/">d</a>"#.into()),
        );
        fake.page(
            "/centralservicos/baseconhecimento/dashboard/rh/",
            Ok(r#"<a href="/centralservicos/baseconhecimento/1/">a</a>"#.into()),
        );
        fake.page(
            "/centralservicos/baseconhecimento/1/",
            Ok(entry_html("Wi-Fi", "01/01/2026 10:00", "rede sem fio")),
        );
        fake.page(
            "/centralservicos/baseconhecimento/2/",
            Err(FetchError::Inaccessible),
        );
        fake.page(
            "/centralservicos/baseconhecimento/3/",
            Err(FetchError::Other("500".into())),
        );
        fake.page(
            "/centralservicos/baseconhecimento/4/",
            Ok("<p>sem título</p>".into()),
        );
        fake.file("/media/guia.txt", Ok(b"manual do roteador".to_vec()));
        fake.file("/media/falha.bin", Err(FetchError::Other("fora".into())));
        fake
    }

    fn setup() -> (tempfile::TempDir, Store, Index) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        let index = crate::index::open(&store).unwrap();
        (dir, store, index)
    }

    fn options() -> UpdateOptions {
        UpdateOptions {
            attachments: true,
            ..UpdateOptions::default()
        }
    }

    fn run(
        fake: &Fake,
        store: &Store,
        index: &mut Index,
        options: &UpdateOptions,
    ) -> Result<Report, Error> {
        update(
            fake,
            store,
            index,
            options,
            "https://suap.exemplo.br/",
            &mut |_| {},
        )
    }

    #[test]
    fn downloads_new_entries_then_skips_unchanged_ones() {
        let (_dir, store, mut index) = setup();
        let fake = world();
        let mut lines = Vec::new();
        let first = update(
            &fake,
            &store,
            &mut index,
            &options(),
            "https://suap.exemplo.br/",
            &mut |l| lines.push(l.to_owned()),
        )
        .unwrap();
        assert_eq!((first.areas, first.listed, first.added), (2, 4, 1));
        assert_eq!(
            (first.inaccessible.clone(), first.failed.clone()),
            (vec![2], vec![3, 4])
        );
        assert_eq!((first.attachments, first.attachment_failures), (1, 1));
        assert!(lines.iter().any(|l| l == "entrada 1: nova") && lines[0].starts_with("4 entrada"));
        let md = std::fs::read_to_string(store.entry_file(1).unwrap().unwrap()).unwrap();
        assert!(md.contains("visibilidade: \"privada\"") && md.contains("../anexos/1/guia.txt"));
        assert_eq!(store.manifest().unwrap()[&1].attachments, vec!["guia.txt"]);
        let hits = crate::index::search(&index, &store.manifest().unwrap(), "roteador", 3).unwrap();
        assert_eq!(hits[0].source, "guia.txt");

        let second = run(&fake, &store, &mut index, &options()).unwrap();
        assert_eq!((second.added, second.updated, second.unchanged), (0, 0, 1));
        let forced = run(
            &fake,
            &store,
            &mut index,
            &UpdateOptions {
                force: true,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(forced.updated, 1);

        fake.page(
            "/centralservicos/baseconhecimento/1/",
            Ok(entry_html("Wi-Fi novo", "05/05/2026 09:00", "outra coisa")),
        );
        let third = run(
            &fake,
            &store,
            &mut index,
            &UpdateOptions {
                attachments: false,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(third.updated, 1);
        assert!(store.manifest().unwrap()[&1].attachments.is_empty());
        assert!(
            crate::index::search(&index, &store.manifest().unwrap(), "roteador", 3)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn only_ids_skips_discovery() {
        let (_dir, store, mut index) = setup();
        let fake = world();
        let report = run(
            &fake,
            &store,
            &mut index,
            &UpdateOptions {
                only: Some(vec![1]),
                ..options()
            },
        )
        .unwrap();
        assert_eq!((report.areas, report.listed, report.added), (0, 1, 1));
    }

    #[test]
    fn prunes_only_after_a_complete_listing() {
        let (_dir, store, mut index) = setup();
        let fake = world();
        run(&fake, &store, &mut index, &options()).unwrap();
        fake.page(
            "/centralservicos/baseconhecimento/dashboard/ti/",
            Ok("<p>vazio</p>".into()),
        );
        fake.page(
            "/centralservicos/baseconhecimento/dashboard/rh/",
            Err(FetchError::Inaccessible),
        );
        let denied = run(
            &fake,
            &store,
            &mut index,
            &UpdateOptions {
                prune: true,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(
            (denied.areas_denied.clone(), denied.removed),
            (vec!["rh".to_owned()], 0)
        );
        assert!(store.entry_file(1).unwrap().is_some());

        fake.page(
            "/centralservicos/baseconhecimento/dashboard/rh/",
            Ok("<p>vazio</p>".into()),
        );
        let pruned = run(
            &fake,
            &store,
            &mut index,
            &UpdateOptions {
                prune: true,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(pruned.removed, 1);
        assert!(store.entry_file(1).unwrap().is_none() && store.manifest().unwrap().is_empty());
        assert!(
            crate::index::search(&index, &store.manifest().unwrap(), "rede", 3)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn expired_session_aborts_in_every_step() {
        let (_dir, store, mut index) = setup();
        let expired = Fake::default();
        expired.page(
            "/centralservicos/baseconhecimento/",
            Err(FetchError::NotAuthenticated),
        );
        assert!(matches!(
            run(&expired, &store, &mut index, &options()),
            Err(Error::Fetch(FetchError::NotAuthenticated))
        ));

        let area = world();
        area.page(
            "/centralservicos/baseconhecimento/dashboard/ti/",
            Err(FetchError::NotAuthenticated),
        );
        assert!(run(&area, &store, &mut index, &options()).is_err());

        let entry = world();
        entry.page(
            "/centralservicos/baseconhecimento/1/",
            Err(FetchError::NotAuthenticated),
        );
        assert!(run(&entry, &store, &mut index, &options()).is_err());

        let file = world();
        file.file("/media/guia.txt", Err(FetchError::NotAuthenticated));
        assert!(run(&file, &store, &mut index, &options()).is_err());
    }
}
