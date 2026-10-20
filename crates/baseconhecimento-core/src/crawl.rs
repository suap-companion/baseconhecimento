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
use std::collections::BTreeSet;

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
    /// Procura também entradas que nenhuma área lista, parando após tantas negativas seguidas.
    pub scan: Option<usize>,
}

/// O resultado da rodada.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub areas: usize,
    pub listed: usize,
    /// Ids tentados pela varredura.
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub attachments: usize,
    /// Imagens do texto baixadas e as que falharam.
    pub images: usize,
    pub image_failures: usize,
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

/// Baixa as imagens do texto para `anexos/<id>/imagens/` e troca, no Markdown, o endereço do SUAP
/// pelo caminho local (relativo à pasta `entradas/`). A que falhar continua apontando para o SUAP.
fn download_images(
    fetcher: &dyn Fetcher,
    store: &Store,
    entry: &mut crate::Entry,
    report: &mut Report,
) -> Result<(), Error> {
    let mut names: Vec<String> = Vec::new();
    for url in entry.images.clone() {
        match fetcher.file(&url) {
            Ok(bytes) => {
                let name = sanitize::unique(&sanitize::name_from_url(&url), &names);
                store.save_image(entry.id, &name, &bytes)?;
                let local = format!(
                    "../anexos/{}/imagens/{}",
                    entry.id,
                    name.replace(' ', "%20")
                );
                entry.summary = entry
                    .summary
                    .replace(&format!("]({url})"), &format!("]({local})"));
                entry.solution = entry
                    .solution
                    .replace(&format!("]({url})"), &format!("]({local})"));
                names.push(name);
                report.images += 1;
            }
            Err(FetchError::NotAuthenticated) => return Err(FetchError::NotAuthenticated.into()),
            Err(_) => report.image_failures += 1,
        }
    }
    Ok(())
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
    let mut entry = match parse::parse_entry(id, &html) {
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
        let names = download_attachments(fetcher, store, &entry, report)?;
        download_images(fetcher, store, &mut entry, report)?;
        names
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

/// Uma rodada em andamento: junta o que `sync_entry` precisa e conta o resultado.
struct Run<'a> {
    fetcher: &'a dyn Fetcher,
    store: &'a Store,
    index: &'a mut Index,
    manifest: Manifest,
    options: &'a UpdateOptions,
    base_url: &'a str,
    report: Report,
    progress: &'a mut dyn FnMut(&str),
}

impl Run<'_> {
    /// Confere uma entrada, conta o resultado e diz se ela existe para esta conta.
    fn entry(&mut self, id: u64, show_misses: bool) -> Result<bool, Error> {
        let step = sync_entry(
            self.fetcher,
            self.store,
            self.index,
            &mut self.manifest,
            self.options,
            self.base_url,
            id,
            &mut self.report,
        )?;
        let hit = match step {
            Step::Added => {
                self.report.added += 1;
                true
            }
            Step::Updated => {
                self.report.updated += 1;
                true
            }
            Step::Unchanged => {
                self.report.unchanged += 1;
                true
            }
            Step::Inaccessible => {
                self.report.inaccessible.push(id);
                false
            }
            Step::Failed => {
                self.report.failed.push(id);
                false
            }
        };
        if hit || show_misses {
            (self.progress)(&format!("entrada {id}: {}", label(&step)));
        }
        Ok(hit)
    }

    /// Procura entradas que nenhuma área lista: os buracos entre os ids conhecidos e os ids depois
    /// do maior, até `stop` negativas seguidas. Os ids que o SUAP negou ficam em `varredura.json`
    /// para não serem tentados de novo (a não ser com `force`).
    fn scan(&mut self, listed: &[u64], stop: usize) -> Result<Vec<u64>, Error> {
        let mut missing = self.store.missing()?;
        let known: Vec<u64> = listed.iter().chain(self.manifest.keys()).copied().collect();
        let top = known.iter().copied().max().unwrap_or(0);
        let skip = |id: &u64, run: &Self, missing: &BTreeSet<u64>| {
            known.contains(id) || (!run.options.force && missing.contains(id))
        };
        let mut found = Vec::new();
        let rechecks: Vec<u64> = self
            .manifest
            .keys()
            .filter(|id| !listed.contains(id))
            .copied()
            .collect();
        for id in rechecks {
            if self.entry(id, true)? {
                found.push(id);
            }
        }
        let mut misses = 0;
        let mut id = 1;
        while id <= top || misses < stop {
            if !skip(&id, self, &missing) {
                if self.entry(id, false)? {
                    found.push(id);
                    misses = 0;
                } else {
                    missing.insert(id);
                    misses += usize::from(id > top);
                }
                self.report.scanned += 1;
            } else if id > top {
                misses += 1;
            }
            id += 1;
        }
        self.store.save_missing(&missing)?;
        Ok(found)
    }
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
    let mut run = Run {
        fetcher,
        store,
        index,
        manifest: store.manifest()?,
        options,
        base_url,
        report: Report::default(),
        progress,
    };
    let mut ids = match &options.only {
        Some(only) => only.clone(),
        None => discover(fetcher, &mut run.report)?,
    };
    run.report.listed = ids.len();
    (run.progress)(&format!("{} entrada(s) para conferir", ids.len()));
    for id in ids.clone() {
        run.entry(id, true)?;
    }
    if let (Some(stop), None) = (options.scan, &options.only) {
        (run.progress)("varrendo ids fora das áreas");
        let found = run.scan(&ids, stop)?;
        ids.extend(found);
    }
    let complete = options.only.is_none() && run.report.areas_denied.is_empty();
    if options.prune && complete {
        let gone: Vec<u64> = run
            .manifest
            .keys()
            .filter(|id| !ids.contains(id) && options.scan.is_none())
            .chain(
                run.report
                    .inaccessible
                    .iter()
                    .filter(|id| run.manifest.contains_key(id)),
            )
            .copied()
            .collect();
        for id in gone {
            store.remove(id)?;
            remove_entry(run.index, id)?;
            run.manifest.remove(&id);
            run.report.removed += 1;
        }
        store.save_manifest(&run.manifest)?;
    }
    Ok(run.report)
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
            <div id="resumo"><div class="accordion-body"><p>{body}</p><img src="/media/foto%20um.png" alt="foto"><img src="/media/foto%20um.png"><img src="/media/outra.png" alt="outra"><img src="/media/quebrada.png" alt="q"></div></div>
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
        fake.file("/media/foto%20um.png", Ok(b"png".to_vec()));
        fake.file("/media/outra.png", Ok(b"png2".to_vec()));
        fake.file("/media/quebrada.png", Err(FetchError::Other("fora".into())));
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
        assert_eq!((first.images, first.image_failures), (2, 1));
        assert!(lines.iter().any(|l| l == "entrada 1: nova") && lines[0].starts_with("4 entrada"));
        let md = std::fs::read_to_string(store.entry_file(1).unwrap().unwrap()).unwrap();
        assert!(md.contains("visibilidade: \"privada\"") && md.contains("../anexos/1/guia.txt"));
        assert!(md.contains("![foto](../anexos/1/imagens/foto%20um.png)"));
        assert!(
            md.contains("![outra](../anexos/1/imagens/outra.png)")
                && md.contains("![q](/media/quebrada.png)")
        );
        assert!(store.images_dir(1).join("foto um.png").exists());
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

        let image = world();
        image.file("/media/outra.png", Err(FetchError::NotAuthenticated));
        assert!(run(&image, &store, &mut index, &options()).is_err());
    }

    #[test]
    fn scan_finds_entries_no_area_lists_and_remembers_misses() {
        let (_dir, store, mut index) = setup();
        let fake = world();
        fake.page(
            "/centralservicos/baseconhecimento/6/",
            Ok(entry_html(
                "Escondida",
                "02/02/2026 10:00",
                "fora das áreas",
            )),
        );
        let scan = |stop| UpdateOptions {
            scan: Some(stop),
            ..options()
        };
        let first = run(&fake, &store, &mut index, &scan(3)).unwrap();
        assert_eq!(first.added, 2);
        assert_eq!(first.scanned, 5);
        assert!(store.manifest().unwrap().contains_key(&6));
        assert_eq!(
            store.missing().unwrap().into_iter().collect::<Vec<_>>(),
            vec![5, 7, 8, 9]
        );

        let again = run(&fake, &store, &mut index, &scan(3)).unwrap();
        assert_eq!((again.scanned, again.added, again.unchanged), (0, 0, 2));
        let forced = run(
            &fake,
            &store,
            &mut index,
            &UpdateOptions {
                force: true,
                ..scan(2)
            },
        )
        .unwrap();
        assert!(forced.scanned >= 2);
    }

    #[test]
    fn scan_fills_holes_and_prunes_what_vanished() {
        let (_dir, store, mut index) = setup();
        let fake = world();
        fake.page(
            "/centralservicos/baseconhecimento/dashboard/ti/",
            Ok(r#"<a href="/centralservicos/baseconhecimento/1/">a</a><a href="/centralservicos/baseconhecimento/5/">e</a>"#.into()),
        );
        fake.page(
            "/centralservicos/baseconhecimento/5/",
            Ok(entry_html("Cinco", "03/03/2026 10:00", "cinco")),
        );
        fake.page(
            "/centralservicos/baseconhecimento/3/",
            Ok(entry_html("Três", "03/03/2026 10:00", "tres")),
        );
        let scan = UpdateOptions {
            scan: Some(1),
            prune: true,
            ..options()
        };
        let first = run(&fake, &store, &mut index, &scan).unwrap();
        assert_eq!(first.added, 3);
        assert_eq!(
            store.missing().unwrap().into_iter().collect::<Vec<_>>(),
            vec![2, 4, 6]
        );

        fake.page(
            "/centralservicos/baseconhecimento/3/",
            Err(FetchError::Inaccessible),
        );
        let pruned = run(&fake, &store, &mut index, &scan).unwrap();
        assert_eq!(pruned.removed, 1);
        assert!(!store.manifest().unwrap().contains_key(&3));
    }

    #[test]
    fn scan_is_ignored_with_only_ids() {
        let (_dir, store, mut index) = setup();
        let report = run(
            &world(),
            &store,
            &mut index,
            &UpdateOptions {
                only: Some(vec![1]),
                scan: Some(5),
                ..options()
            },
        )
        .unwrap();
        assert_eq!(report.scanned, 0);
    }
}
