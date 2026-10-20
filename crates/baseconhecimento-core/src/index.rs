//! Índice de busca offline (SQLite/FTS5, do `suap-rag`) sobre as entradas e os anexos de texto/PDF.
//!
//! No índice, cada arquivo é um documento chamado `<id>-<nome>`: assim se acha, pelo prefixo, tudo
//! o que pertence a uma entrada.

use std::{fs, path::Path};

use suap_rag::{Index, Kind};

use crate::{
    store::{Manifest, Store},
    Error,
};

/// Abre (ou cria) o índice da cópia local.
pub fn open(store: &Store) -> Result<Index, Error> {
    Ok(Index::open(&store.index_path())?)
}

fn add_file(index: &mut Index, name: &str, path: &Path, now: i64) -> Result<(), Error> {
    let size = fs::metadata(path)?.len();
    let id = index.add_document(name, &path.to_string_lossy(), size, now)?;
    index.index_document(id, &mut |_, _| true)?;
    Ok(())
}

/// Tira do índice tudo o que é da entrada (o arquivo em disco não é tocado).
pub fn remove_entry(index: &Index, id: u64) -> Result<(), Error> {
    let prefix = format!("{id}-");
    for document in index.list()? {
        if document.name.starts_with(&prefix) {
            index.remove(document.id)?;
        }
    }
    Ok(())
}

/// Reindexa a entrada: o Markdown e os anexos que o índice sabe ler (TXT, MD e PDF).
pub fn reindex_entry(
    index: &mut Index,
    store: &Store,
    id: u64,
    attachments: &[String],
    now: i64,
) -> Result<(), Error> {
    remove_entry(index, id)?;
    if let Some(file) = store.entry_file(id)? {
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
        add_file(index, &name.unwrap_or_default(), &file, now)?;
    }
    for name in attachments.iter().filter(|name| Kind::of(name).is_some()) {
        let path = store.attachments_dir(id).join(name);
        add_file(index, &format!("{id}-anexo-{name}"), &path, now)?;
    }
    Ok(())
}

/// Refaz o índice inteiro a partir do manifesto.
pub fn rebuild(
    index: &mut Index,
    store: &Store,
    manifest: &Manifest,
    now: i64,
) -> Result<usize, Error> {
    for document in index.list()? {
        index.remove(document.id)?;
    }
    for (id, record) in manifest {
        reindex_entry(index, store, *id, &record.attachments, now)?;
    }
    Ok(manifest.len())
}

/// Um trecho achado, ligado à entrada de origem.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub id: u64,
    pub title: String,
    pub visibility: Option<crate::Visibility>,
    pub area: Option<String>,
    /// De onde veio o trecho: o Markdown da entrada ou o nome do anexo.
    pub source: String,
    pub page: Option<u32>,
    pub text: String,
    pub score: f64,
}

/// O id da entrada de um documento do índice (`<id>-...`).
fn entry_id(document_name: &str) -> Option<u64> {
    document_name.split('-').next()?.parse().ok()
}

/// Busca por palavras (FTS5/BM25), sem acentos nem caixa, nos textos de todas as entradas.
pub fn search(
    index: &Index,
    manifest: &Manifest,
    query: &str,
    limit: usize,
) -> Result<Vec<Found>, Error> {
    let mut found = Vec::new();
    for hit in index.search(query, limit)? {
        let Some(id) = entry_id(&hit.doc_name) else {
            continue;
        };
        let Some(record) = manifest.get(&id) else {
            continue;
        };
        let source = match hit.doc_name.split_once("-anexo-") {
            Some((_, attachment)) => attachment.to_owned(),
            None => "entrada".to_owned(),
        };
        found.push(Found {
            id,
            title: record.title.clone(),
            visibility: record.visibility,
            area: record.area.clone(),
            source,
            page: hit.page,
            text: hit.text,
            score: hit.score,
        });
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Visibility, store::Record};

    fn record(attachments: &[&str]) -> Record {
        Record {
            title: "Wi-Fi".into(),
            area: Some("TI".into()),
            visibility: Some(Visibility::Private),
            tags: vec![],
            updated_at: None,
            hash: String::new(),
            fetched_at: 0,
            attachments: attachments.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    fn entry(id: u64, title: &str) -> crate::Entry {
        crate::Entry {
            id,
            title: title.into(),
            area: None,
            visibility: None,
            tags: vec![],
            active: None,
            needs_correction: false,
            pending_review: false,
            updated_at: None,
            updated_by: None,
            services: vec![],
            groups: vec![],
            attachments: vec![],
            summary: String::new(),
            solution: String::new(),
        }
    }

    #[test]
    fn indexes_searches_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        let mut index = open(&store).unwrap();
        store
            .save_entry(
                &entry(1, "Wi-Fi"),
                "# Wi-Fi\n\nConfigurar a rede sem fio da biblioteca.",
            )
            .unwrap();
        store
            .save_entry(
                &entry(10, "Outra"),
                "# Outra\n\nImpressora travada na secretaria.",
            )
            .unwrap();
        store
            .save_attachment(1, "guia.txt", "Senha temporária do roteador.".as_bytes())
            .unwrap();
        store.save_attachment(1, "foto.png", b"png").unwrap();
        let mut manifest = Manifest::new();
        manifest.insert(1, record(&["guia.txt", "foto.png"]));
        manifest.insert(
            10,
            Record {
                title: "Outra".into(),
                ..record(&[])
            },
        );

        assert_eq!(rebuild(&mut index, &store, &manifest, 1).unwrap(), 2);
        let by_body = search(&index, &manifest, "biblioteca", 5).unwrap();
        assert_eq!((by_body[0].id, by_body[0].source.as_str()), (1, "entrada"));
        assert_eq!(by_body[0].title, "Wi-Fi");
        assert_eq!(by_body[0].visibility, Some(Visibility::Private));
        assert_eq!(by_body[0].area.as_deref(), Some("TI"));
        let by_attachment = search(&index, &manifest, "roteador", 5).unwrap();
        assert_eq!(by_attachment[0].source, "guia.txt");
        assert!(
            by_attachment[0].page.is_none()
                && by_attachment[0].score != 0.0
                && !by_attachment[0].text.is_empty()
        );

        remove_entry(&index, 1).unwrap();
        assert!(search(&index, &manifest, "biblioteca", 5)
            .unwrap()
            .is_empty());
        assert_eq!(
            search(&index, &manifest, "impressora", 5).unwrap()[0].id,
            10
        );
        // Reindexar uma entrada sem arquivo em disco só limpa o índice.
        reindex_entry(&mut index, &store, 99, &[], 2).unwrap();
    }

    #[test]
    fn ignores_hits_without_a_record() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        let mut index = open(&store).unwrap();
        store
            .save_entry(&entry(3, "Sem registro"), "palavra rara zumbido")
            .unwrap();
        reindex_entry(&mut index, &store, 3, &[], 1).unwrap();
        assert!(search(&index, &Manifest::new(), "zumbido", 5)
            .unwrap()
            .is_empty());
        let stray = dir.path().join("solto.md");
        fs::write(&stray, "palavra rara zumbido").unwrap();
        add_file(&mut index, "solto.md", &stray, 1).unwrap();
        assert!(search(&index, &Manifest::new(), "zumbido", 5)
            .unwrap()
            .is_empty());
        assert_eq!(entry_id("12-x.md"), Some(12));
        assert_eq!(entry_id("x.md"), None);
    }
}
