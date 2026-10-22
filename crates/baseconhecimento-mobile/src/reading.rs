//! Leitura offline da cópia local: lista, filtros, uma entrada por inteiro e a busca.

use std::{collections::BTreeSet, fs, path::Path};

use baseconhecimento_core::{
    index,
    store::{Record, Store},
    Visibility,
};
use serde_json::Value;

use crate::{convert, failed, Base, BaseError};

/// Quem enxerga a entrada no SUAP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EntryVisibility {
    Public,
    Private,
    Confidential,
}

impl From<Visibility> for EntryVisibility {
    fn from(visibility: Visibility) -> Self {
        match visibility {
            Visibility::Public => Self::Public,
            Visibility::Private => Self::Private,
            Visibility::Confidential => Self::Confidential,
        }
    }
}

impl From<EntryVisibility> for Visibility {
    fn from(visibility: EntryVisibility) -> Self {
        match visibility {
            EntryVisibility::Public => Self::Public,
            EntryVisibility::Private => Self::Private,
            EntryVisibility::Confidential => Self::Confidential,
        }
    }
}

/// Uma entrada da lista (sem o texto).
#[derive(Debug, Clone, uniffi::Record)]
pub struct EntrySummary {
    pub id: u64,
    pub title: String,
    pub area: Option<String>,
    pub visibility: Option<EntryVisibility>,
    pub tags: Vec<String>,
    pub updated_at: Option<String>,
    /// Segundos Unix do último download.
    pub fetched_at: i64,
    pub attachments: u32,
}

/// O que a lista mostra; tudo é opcional e o que for dado se combina (E).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct EntryFilter {
    /// Parte do título, da área ou de uma tag (sem diferenciar caixa nem acentos).
    pub text: Option<String>,
    /// O nome exato da área.
    pub area: Option<String>,
    pub visibility: Option<EntryVisibility>,
    /// A entrada precisa ter todas estas tags.
    pub tags: Vec<String>,
}

/// As áreas e tags que existem na cópia local, para montar os filtros.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FilterOptions {
    pub areas: Vec<String>,
    pub tags: Vec<String>,
}

/// Um arquivo anexado, já baixado.
#[derive(Debug, Clone, uniffi::Record)]
pub struct AttachmentFile {
    pub name: String,
    /// O caminho do arquivo no aparelho.
    pub path: String,
    pub size: u64,
}

/// A entrada por inteiro.
#[derive(Debug, Clone, uniffi::Record)]
pub struct EntryDetail {
    pub summary: EntrySummary,
    /// O Markdown do Resumo, da Solução e dos Anexos, sem o frontmatter nem o título.
    pub markdown: String,
    /// A pasta a que os caminhos relativos do Markdown (imagens e anexos) se referem.
    pub entries_dir: String,
    /// A pasta da cópia local: nenhum arquivo fora dela deve ser aberto.
    pub root_dir: String,
    /// O endereço da entrada no SUAP.
    pub url: Option<String>,
    pub updated_by: Option<String>,
    pub services: Vec<String>,
    pub groups: Vec<String>,
    /// `false` quando o SUAP mostra "Inativo".
    pub active: bool,
    pub needs_correction: bool,
    pub pending_review: bool,
    pub attachments: Vec<AttachmentFile>,
}

/// Um trecho achado pela busca.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchHit {
    pub id: u64,
    pub title: String,
    pub area: Option<String>,
    pub visibility: Option<EntryVisibility>,
    /// De onde veio o trecho: `entrada` ou o nome do anexo.
    pub source: String,
    pub page: Option<u32>,
    pub snippet: String,
}

/// Minúsculas e sem os acentos do português, para comparar textos.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

impl EntryFilter {
    fn matches(&self, record: &Record) -> bool {
        let by_text = self.text.as_deref().is_none_or(|text| {
            let needle = fold(text.trim());
            [Some(&record.title), record.area.as_ref()]
                .into_iter()
                .flatten()
                .chain(&record.tags)
                .any(|field| fold(field).contains(&needle))
        });
        by_text
            && self
                .area
                .as_ref()
                .is_none_or(|a| record.area.as_ref() == Some(a))
            && self
                .visibility
                .is_none_or(|v| record.visibility == Some(v.into()))
            && self.tags.iter().all(|tag| record.tags.contains(tag))
    }
}

fn summary(id: u64, record: &Record) -> EntrySummary {
    EntrySummary {
        id,
        title: record.title.clone(),
        area: record.area.clone(),
        visibility: record.visibility.map(Into::into),
        tags: record.tags.clone(),
        updated_at: record.updated_at.clone(),
        fetched_at: record.fetched_at,
        attachments: record.attachments.len() as u32,
    }
}

/// Separa o frontmatter (`chave: valor`, com valores em JSON) do corpo do Markdown.
fn split_front(text: &str) -> (Vec<(String, Value)>, &str) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (Vec::new(), text);
    };
    let Some((front, body)) = rest.split_once("\n---\n") else {
        return (Vec::new(), text);
    };
    let pairs = front
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(": ")?;
            let value = serde_json::from_str(value).unwrap_or(Value::Null);
            Some((key.to_owned(), value))
        })
        .collect();
    (pairs, body)
}

/// Tira o título (`# ...`) do começo, que o app já mostra no cabeçalho.
fn without_title(body: &str) -> &str {
    let body = body.trim_start();
    let rest = match body.strip_prefix("# ") {
        Some(rest) => rest.split_once('\n').map_or("", |(_, after)| after),
        None => body,
    };
    rest.trim()
}

fn text_of(front: &[(String, Value)], key: &str) -> Option<String> {
    front
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| value.as_str().map(str::to_owned))
}

fn list_of(front: &[(String, Value)], key: &str) -> Vec<String> {
    let found = front.iter().find(|(name, _)| name == key);
    let items = found.and_then(|(_, value)| value.as_array());
    items
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn flag_of(front: &[(String, Value)], key: &str, default: bool) -> bool {
    let found = front.iter().find(|(name, _)| name == key);
    found
        .and_then(|(_, value)| value.as_bool())
        .unwrap_or(default)
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Base {
    fn store(&self) -> Result<Store, BaseError> {
        Store::open(self.root.clone()).map_err(convert)
    }
}

#[uniffi::export]
impl Base {
    /// As entradas da cópia local que passam pelo filtro, em ordem alfabética do título.
    pub fn entries(&self, filter: EntryFilter) -> Result<Vec<EntrySummary>, BaseError> {
        let manifest = self.store()?.manifest().map_err(convert)?;
        let mut rows: Vec<EntrySummary> = manifest
            .iter()
            .filter(|(_, record)| filter.matches(record))
            .map(|(id, record)| summary(*id, record))
            .collect();
        rows.sort_by_cached_key(|row| fold(&row.title));
        Ok(rows)
    }

    /// As áreas e tags da cópia local, em ordem alfabética.
    pub fn filter_options(&self) -> Result<FilterOptions, BaseError> {
        let manifest = self.store()?.manifest().map_err(convert)?;
        let mut areas = BTreeSet::new();
        let mut tags = BTreeSet::new();
        for record in manifest.values() {
            areas.extend(record.area.clone());
            tags.extend(record.tags.iter().cloned());
        }
        Ok(FilterOptions {
            areas: areas.into_iter().collect(),
            tags: tags.into_iter().collect(),
        })
    }

    /// A entrada por inteiro, lida do arquivo Markdown (sem rede).
    pub fn entry(&self, id: u64) -> Result<EntryDetail, BaseError> {
        let store = self.store()?;
        let manifest = store.manifest().map_err(convert)?;
        let file = store.entry_file(id).map_err(convert)?;
        let (Some(record), Some(file)) = (manifest.get(&id), file) else {
            return Err(failed(format!(
                "a entrada {id} não está na cópia local: atualize a base"
            )));
        };
        let text = fs::read_to_string(&file).map_err(failed)?;
        let (front, body) = split_front(&text);
        let folder = store.attachments_dir(id);
        let attachments = record
            .attachments
            .iter()
            .filter_map(|name| {
                let size = fs::metadata(folder.join(name)).ok()?.len();
                Some(AttachmentFile {
                    name: name.clone(),
                    path: display(&folder.join(name)),
                    size,
                })
            })
            .collect();
        Ok(EntryDetail {
            summary: summary(id, record),
            markdown: without_title(body).to_owned(),
            entries_dir: display(&store.root().join("entradas")),
            root_dir: display(store.root()),
            url: text_of(&front, "url"),
            updated_by: text_of(&front, "atualizado_por"),
            services: list_of(&front, "servicos"),
            groups: list_of(&front, "grupos"),
            active: flag_of(&front, "ativo", true),
            needs_correction: flag_of(&front, "necessita_correcao", false),
            pending_review: flag_of(&front, "supervisao_pendente", false),
            attachments,
        })
    }

    /// Busca por palavras (sem acentos nem caixa) nas entradas e nos anexos TXT, MD e PDF, uma
    /// ocorrência por entrada, da mais relevante à menos. O filtro vale também aqui.
    pub fn search(
        &self,
        query: String,
        filter: EntryFilter,
        limit: u32,
    ) -> Result<Vec<SearchHit>, BaseError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let store = self.store()?;
        let manifest = store.manifest().map_err(convert)?;
        let search_index = index::open(&store).map_err(convert)?;
        let wide = (limit as usize).saturating_mul(5).max(50);
        let found = index::search(&search_index, &manifest, &query, wide).map_err(convert)?;
        let mut seen = BTreeSet::new();
        Ok(found
            .into_iter()
            .filter(|hit| filter.matches(&manifest[&hit.id]) && seen.insert(hit.id))
            .take(limit as usize)
            .map(|hit| SearchHit {
                id: hit.id,
                title: hit.title,
                area: hit.area,
                visibility: hit.visibility.map(Into::into),
                source: hit.source,
                page: hit.page,
                snippet: hit.text.split_whitespace().collect::<Vec<_>>().join(" "),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{serial, Env};

    fn loaded() -> (Env, std::sync::Arc<Base>) {
        let _turn = serial();
        let env = Env::new();
        env.suap();
        let base = env.base();
        base.import_session_cookies("sessionid=abc".into()).unwrap();
        base.update(crate::updating::UpdateParams::default())
            .unwrap();
        (env, base)
    }

    fn filter() -> EntryFilter {
        EntryFilter::default()
    }

    #[test]
    fn visibility_converts_both_ways() {
        for visibility in [
            Visibility::Public,
            Visibility::Private,
            Visibility::Confidential,
        ] {
            assert_eq!(
                Visibility::from(EntryVisibility::from(visibility)),
                visibility
            );
        }
    }

    #[test]
    fn folds_case_and_accents() {
        assert_eq!(fold("ÁÉíÕÜ Çã"), "aeiou ca");
    }

    #[test]
    fn lists_and_filters_the_local_copy() {
        let (_env, base) = loaded();
        let all = base.entries(filter()).unwrap();
        let titles: Vec<_> = all.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Solicitar conta", "Trocar o toner"]);
        assert_eq!(all[1].attachments, 1);
        let text = |text: &str| EntryFilter {
            text: Some(text.into()),
            ..filter()
        };
        assert_eq!(base.entries(text("TONER")).unwrap().len(), 1);
        assert_eq!(base.entries(text("tecnologia")).unwrap().len(), 1);
        assert_eq!(base.entries(text("impressora")).unwrap().len(), 1);
        assert!(base.entries(text("zzz")).unwrap().is_empty());
        let area = |name: &str| EntryFilter {
            area: Some(name.into()),
            ..filter()
        };
        assert_eq!(base.entries(area("Tecnologia")).unwrap().len(), 1);
        assert!(base.entries(area("Outra")).unwrap().is_empty());
        let private = EntryFilter {
            visibility: Some(EntryVisibility::Private),
            ..filter()
        };
        assert_eq!(base.entries(private).unwrap()[0].id, 1);
        let tagged = EntryFilter {
            tags: vec!["toner".into(), "impressora".into()],
            ..filter()
        };
        assert_eq!(base.entries(tagged).unwrap().len(), 1);
        let missing = EntryFilter {
            tags: vec!["toner".into(), "outra".into()],
            ..filter()
        };
        assert!(base.entries(missing).unwrap().is_empty());
        let options = base.filter_options().unwrap();
        assert_eq!(options.areas, ["Tecnologia"]);
        assert_eq!(options.tags, ["impressora", "toner"]);
    }

    #[test]
    fn reads_an_entry_with_its_attachments() {
        let (_env, base) = loaded();
        let entry = base.entry(1).unwrap();
        assert_eq!(entry.summary.title, "Trocar o toner");
        assert_eq!(entry.summary.visibility, Some(EntryVisibility::Private));
        assert!(entry.markdown.starts_with("## Resumo"));
        assert!(entry.markdown.contains("](../anexos/1/guia.txt)"));
        assert!(entry
            .url
            .unwrap()
            .ends_with("/centralservicos/baseconhecimento/1/"));
        assert!(entry.active && !entry.needs_correction && !entry.pending_review);
        assert!(entry.services.is_empty() && entry.groups.is_empty());
        assert_eq!(entry.updated_by.as_deref(), Some("Pessoa"));
        assert_eq!(entry.attachments.len(), 1);
        assert_eq!(entry.attachments[0].name, "guia.txt");
        assert_eq!(entry.attachments[0].size, 26);
        let read = fs::read_to_string(&entry.attachments[0].path).unwrap();
        assert!(read.contains("toner"));
        assert!(Path::new(&entry.entries_dir).starts_with(&entry.root_dir));
        // O arquivo que sumiu do disco não aparece como anexo.
        fs::remove_file(&entry.attachments[0].path).unwrap();
        assert!(base.entry(1).unwrap().attachments.is_empty());
        // Entrada que não foi baixada.
        let error = base.entry(3).unwrap_err().to_string();
        assert!(error.contains("não está na cópia local"));
    }

    #[test]
    fn an_unreadable_entry_file_is_an_error() {
        let (_env, base) = loaded();
        let file = Store::open(base.root.clone())
            .unwrap()
            .entry_file(2)
            .unwrap()
            .unwrap();
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(base.entry(2).is_err());
    }

    #[test]
    fn splits_the_front_matter_and_the_title() {
        let (front, body) = split_front(
            "---\nid: 1\nativo: false\nx: ???\ntags: [\"a\", \"b\"]\n\n---\n\n# T\n\ncorpo",
        );
        assert_eq!(front.len(), 4);
        assert_eq!(list_of(&front, "tags"), ["a", "b"]);
        assert!(!flag_of(&front, "ativo", true));
        assert!(flag_of(&front, "ausente", true));
        assert_eq!(text_of(&front, "id"), None);
        assert!(list_of(&front, "x").is_empty());
        assert_eq!(without_title(body), "corpo");
        assert_eq!(without_title("# Só título"), "");
        assert_eq!(without_title("sem título"), "sem título");
        // Sem frontmatter (ou sem fechamento) o texto vem inteiro.
        assert_eq!(split_front("texto").1, "texto");
        assert_eq!(
            split_front("---\nabre e não fecha").1,
            "---\nabre e não fecha"
        );
    }

    #[test]
    fn searches_entries_and_attachments_once_per_entry() {
        let (_env, base) = loaded();
        let hits = base.search("toner".into(), filter(), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, 1);
        assert_eq!(hits[0].area.as_deref(), Some("Tecnologia"));
        assert_eq!(hits[0].visibility, Some(EntryVisibility::Private));
        assert!(hits[0].snippet.contains("toner"));
        let by_text = base.search("conta".into(), filter(), 10).unwrap();
        assert_eq!(by_text[0].source, "entrada");
        assert!(by_text[0].page.is_none());
        let public = EntryFilter {
            visibility: Some(EntryVisibility::Public),
            ..filter()
        };
        assert!(base.search("toner".into(), public, 10).unwrap().is_empty());
        assert!(base.search("  ".into(), filter(), 10).unwrap().is_empty());
        // Aspas e operadores digitados pela pessoa não podem derrubar a busca.
        for odd in ["\"toner", "toner -", "AND OR", "a*"] {
            assert!(base.search(odd.into(), filter(), 10).is_ok(), "{odd}");
        }
    }
}
