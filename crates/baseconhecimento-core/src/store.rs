//! A cópia local: uma pasta por perfil com as entradas em Markdown, os anexos e o manifesto.
//!
//! ```text
//! <dados>/base/<perfil>/
//!   entradas/<id>-<titulo>.md   texto da entrada, com os metadados no frontmatter
//!   anexos/<id>/<arquivo>       anexos, com nomes sanitizados
//!   manifesto.json              o que foi baixado e quando
//!   indice.sqlite               índice de busca (FTS5)
//!   settings.json               configurações da atualização
//! ```

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    clock,
    model::{Entry, Visibility},
    sanitize, Error,
};

/// Pasta da cópia local do perfil: `<dados>/base/<perfil>`.
pub fn data_root(paths: &suap_core::AppPaths) -> PathBuf {
    paths.data_dir().join("base").join(paths.profile())
}

const MANIFEST: &str = "manifesto.json";
const SCAN_FILE: &str = "varredura.json";

/// O que se sabe de uma entrada já baixada (sem o texto, que fica no arquivo Markdown).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub title: String,
    pub area: Option<String>,
    pub visibility: Option<Visibility>,
    pub tags: Vec<String>,
    pub updated_at: Option<String>,
    /// [`Entry::content_hash`] da última versão baixada.
    #[serde(default)]
    pub hash: String,
    /// Segundos Unix do último download.
    pub fetched_at: i64,
    /// Nomes locais dos anexos baixados.
    pub attachments: Vec<String>,
}

/// Entradas baixadas, por id.
pub type Manifest = BTreeMap<u64, Record>;

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

fn quoted(text: &str) -> String {
    // Texto JSON é texto YAML válido.
    serde_json::to_string(text).unwrap_or_default()
}

/// O arquivo Markdown da entrada: frontmatter YAML e o texto convertido.
pub fn render_markdown(
    entry: &Entry,
    base_url: &str,
    fetched_at: i64,
    local_attachments: &[String],
) -> String {
    let mut front = vec![
        format!("id: {}", entry.id),
        format!("titulo: {}", quoted(&entry.title)),
        format!(
            "url: {}",
            quoted(&format!(
                "{}{}",
                base_url.trim_end_matches('/'),
                crate::parse::entry_path(entry.id)
            ))
        ),
    ];
    let optional = [
        ("area", entry.area.as_deref()),
        ("visibilidade", entry.visibility.map(Visibility::as_str)),
        ("atualizado_em", entry.updated_at.as_deref()),
        ("atualizado_por", entry.updated_by.as_deref()),
    ];
    for (key, value) in optional {
        if let Some(value) = value {
            front.push(format!("{key}: {}", quoted(value)));
        }
    }
    if let Some(active) = entry.active {
        front.push(format!("ativo: {active}"));
    }
    front.push(format!("necessita_correcao: {}", entry.needs_correction));
    front.push(format!("supervisao_pendente: {}", entry.pending_review));
    let lists = [
        ("tags", &entry.tags),
        ("servicos", &entry.services),
        ("grupos", &entry.groups),
        ("anexos", &local_attachments.to_vec()),
    ];
    for (key, values) in lists {
        front.push(format!(
            "{key}: {}",
            serde_json::to_string(values).unwrap_or_default()
        ));
    }
    front.push(format!("baixado_em: {}", quoted(&clock::iso(fetched_at))));

    let mut body = format!("---\n{}\n---\n\n# {}\n", front.join("\n"), entry.title);
    if !entry.summary.is_empty() {
        body.push_str(&format!("\n## Resumo\n\n{}\n", entry.summary));
    }
    if !entry.solution.is_empty() {
        body.push_str(&format!("\n## Solução\n\n{}\n", entry.solution));
    }
    if !local_attachments.is_empty() {
        body.push_str("\n## Anexos\n\n");
        for name in local_attachments {
            body.push_str(&format!(
                "- [{name}](../anexos/{}/{})\n",
                entry.id,
                name.replace(' ', "%20")
            ));
        }
    }
    body
}

impl Store {
    /// Abre (e cria) a pasta do perfil.
    pub fn open(root: PathBuf) -> Result<Self, Error> {
        fs::create_dir_all(root.join("entradas"))?;
        fs::create_dir_all(root.join("anexos"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn index_path(&self) -> PathBuf {
        self.root.join("indice.sqlite")
    }

    pub fn manifest(&self) -> Result<Manifest, Error> {
        let file = self.root.join(MANIFEST);
        if !file.exists() {
            return Ok(Manifest::new());
        }
        Ok(serde_json::from_str(&fs::read_to_string(file)?)?)
    }

    pub fn save_manifest(&self, manifest: &Manifest) -> Result<(), Error> {
        let temporary = self.root.join("manifesto.json.tmp");
        fs::write(&temporary, serde_json::to_string_pretty(manifest)?)?;
        fs::rename(temporary, self.root.join(MANIFEST))?;
        Ok(())
    }

    /// Ids que a varredura já tentou e o SUAP negou ou não achou.
    pub fn missing(&self) -> Result<BTreeSet<u64>, Error> {
        let file = self.root.join(SCAN_FILE);
        if !file.exists() {
            return Ok(BTreeSet::new());
        }
        Ok(serde_json::from_str(&fs::read_to_string(file)?)?)
    }

    pub fn save_missing(&self, missing: &BTreeSet<u64>) -> Result<(), Error> {
        let text = serde_json::to_string(missing)?;
        fs::write(self.root.join(SCAN_FILE), text)?;
        Ok(())
    }

    /// O arquivo Markdown da entrada, se já foi baixada.
    pub fn entry_file(&self, id: u64) -> Result<Option<PathBuf>, Error> {
        let prefix = format!("{id}-");
        for item in fs::read_dir(self.root.join("entradas"))? {
            let path = item?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with(&prefix) && name.ends_with(".md") {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    /// Grava o Markdown da entrada (trocando o arquivo antigo, que pode ter outro título no nome).
    pub fn save_entry(&self, entry: &Entry, markdown: &str) -> Result<PathBuf, Error> {
        if let Some(old) = self.entry_file(entry.id)? {
            fs::remove_file(old)?;
        }
        let slug = sanitize::slug(&entry.title);
        let name = if slug.is_empty() {
            format!("{}-entrada.md", entry.id)
        } else {
            format!("{}-{slug}.md", entry.id)
        };
        let path = self.root.join("entradas").join(name);
        fs::write(&path, markdown)?;
        Ok(path)
    }

    /// Pasta dos anexos da entrada.
    pub fn attachments_dir(&self, id: u64) -> PathBuf {
        self.root.join("anexos").join(id.to_string())
    }

    /// Pasta das imagens do texto da entrada (dentro da pasta dos anexos).
    pub fn images_dir(&self, id: u64) -> PathBuf {
        self.attachments_dir(id).join("imagens")
    }

    /// Grava uma imagem do texto; `name` já deve ter passado por [`sanitize::file_name`].
    pub fn save_image(&self, id: u64, name: &str, bytes: &[u8]) -> Result<PathBuf, Error> {
        let dir = self.images_dir(id);
        fs::create_dir_all(&dir)?;
        let path = dir.join(name);
        fs::write(&path, bytes)?;
        Ok(path)
    }

    /// Apaga os anexos da entrada (antes de baixar de novo).
    pub fn clear_attachments(&self, id: u64) -> Result<(), Error> {
        let dir = self.attachments_dir(id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Grava um anexo; `name` já deve ter passado por [`sanitize::file_name`].
    pub fn save_attachment(&self, id: u64, name: &str, bytes: &[u8]) -> Result<PathBuf, Error> {
        let dir = self.attachments_dir(id);
        fs::create_dir_all(&dir)?;
        let path = dir.join(name);
        fs::write(&path, bytes)?;
        Ok(path)
    }

    /// Apaga a entrada e os anexos do disco.
    pub fn remove(&self, id: u64) -> Result<(), Error> {
        if let Some(file) = self.entry_file(id)? {
            fs::remove_file(file)?;
        }
        self.clear_attachments(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_root_is_per_profile() {
        let paths = suap_core::AppPaths::from_dirs("/c".into(), "/d".into());
        assert_eq!(data_root(&paths), PathBuf::from("/d/base/default"));
        let other = paths.with_profile("local").unwrap();
        assert_eq!(data_root(&other), PathBuf::from("/d/base/local"));
    }

    pub fn sample() -> Entry {
        Entry {
            id: 7,
            title: "Título \"citado\"".into(),
            area: Some("TI".into()),
            visibility: Some(Visibility::Confidential),
            tags: vec!["a".into(), "b".into()],
            active: Some(true),
            needs_correction: false,
            pending_review: true,
            updated_at: Some("01/02/2026 10:00".into()),
            updated_by: Some("Pessoa Fictícia".into()),
            services: vec!["Serviço".into()],
            groups: vec![],
            attachments: vec![],
            images: vec![],
            summary: "Resumo".into(),
            solution: "Solução".into(),
        }
    }

    #[test]
    fn renders_frontmatter_and_sections() {
        let md = render_markdown(
            &sample(),
            "https://suap.exemplo.br/",
            0,
            &["a b.pdf".to_owned()],
        );
        assert!(md.starts_with("---\nid: 7\ntitulo: \"Título \\\"citado\\\"\"\n"));
        assert!(md.contains("url: \"https://suap.exemplo.br/centralservicos/baseconhecimento/7/\""));
        assert!(md.contains("visibilidade: \"sigilosa\"\n"));
        assert!(md.contains("ativo: true\nnecessita_correcao: false\nsupervisao_pendente: true\n"));
        assert!(md.contains("tags: [\"a\",\"b\"]\n") && md.contains("grupos: []\n"));
        assert!(md.contains("baixado_em: \"1970-01-01T00:00:00Z\""));
        assert!(md.contains("\n## Resumo\n\nResumo\n\n## Solução\n\nSolução\n"));
        assert!(md.contains("- [a b.pdf](../anexos/7/a%20b.pdf)\n"));
    }

    #[test]
    fn leaves_out_what_is_missing() {
        let entry = Entry {
            area: None,
            visibility: None,
            active: None,
            updated_at: None,
            updated_by: None,
            summary: String::new(),
            solution: String::new(),
            ..sample()
        };
        let md = render_markdown(&entry, "https://suap.exemplo.br", 0, &[]);
        assert!(!md.contains("area:") && !md.contains("ativo:") && !md.contains("## "));
    }

    #[test]
    fn saves_replaces_and_removes_entries() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("p")).unwrap();
        assert_eq!(store.entry_file(7).unwrap(), None);
        let first = store.save_entry(&sample(), "um").unwrap();
        let renamed = Entry {
            title: "Outro".into(),
            ..sample()
        };
        let second = store.save_entry(&renamed, "dois").unwrap();
        assert!(!first.exists() && second.ends_with("7-outro.md"));
        assert_eq!(store.entry_file(7).unwrap(), Some(second.clone()));
        let blank = Entry {
            id: 8,
            title: "***".into(),
            ..sample()
        };
        assert!(store
            .save_entry(&blank, "x")
            .unwrap()
            .ends_with("8-entrada.md"));
        store.save_attachment(7, "a.txt", b"x").unwrap();
        store.remove(7).unwrap();
        assert!(!second.exists() && !store.attachments_dir(7).exists());
        store.remove(7).unwrap();
    }

    #[test]
    fn manifest_round_trips_and_clears_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().to_path_buf()).unwrap();
        assert!(store.manifest().unwrap().is_empty());
        let mut manifest = Manifest::new();
        manifest.insert(
            1,
            Record {
                title: "t".into(),
                area: None,
                visibility: Some(Visibility::Public),
                tags: vec![],
                updated_at: None,
                hash: String::new(),
                fetched_at: 5,
                attachments: vec![],
            },
        );
        store.save_manifest(&manifest).unwrap();
        assert_eq!(store.manifest().unwrap(), manifest);
        assert!(store.missing().unwrap().is_empty());
        store.save_missing(&BTreeSet::from([4, 9])).unwrap();
        assert_eq!(store.missing().unwrap(), BTreeSet::from([4, 9]));
        store.save_attachment(1, "x.txt", b"x").unwrap();
        assert!(store
            .save_image(1, "i.png", b"x")
            .unwrap()
            .starts_with(store.images_dir(1)));
        store.clear_attachments(1).unwrap();
        assert!(!store.attachments_dir(1).exists());
        assert_eq!(store.root(), dir.path());
        assert!(store.index_path().ends_with("indice.sqlite"));
    }
}
