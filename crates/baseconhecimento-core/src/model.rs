//! Tipos do domínio: a entrada da Base de Conhecimento e o que se guarda dela.

use serde::{Deserialize, Serialize};

/// Quem enxerga a entrada no SUAP (`BaseConhecimento.visibilidade`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// Atendentes e a área de Perguntas Frequentes.
    #[serde(rename = "publica")]
    Public,
    /// Todos os atendentes da Central de Serviços.
    #[serde(rename = "privada")]
    Private,
    /// Só os grupos de atendimento escolhidos.
    #[serde(rename = "sigilosa")]
    Confidential,
}

impl Visibility {
    /// Lê o texto do SUAP (`publica`, `Pública`, `SIGILOSA`...), sem importar a caixa.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "publica" | "pública" => Some(Self::Public),
            "privada" => Some(Self::Private),
            "sigilosa" => Some(Self::Confidential),
            _ => None,
        }
    }

    /// O nome usado nos arquivos e na linha de comando.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "publica",
            Self::Private => "privada",
            Self::Confidential => "sigilosa",
        }
    }
}

/// Um anexo como o SUAP o lista: o endereço e o nome mostrado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentRef {
    /// Endereço (`href`) do arquivo no SUAP.
    pub url: String,
    /// Nome mostrado pelo SUAP.
    pub name: String,
}

/// Uma entrada da Base de Conhecimento, já com o texto em Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub title: String,
    pub area: Option<String>,
    pub visibility: Option<Visibility>,
    pub tags: Vec<String>,
    /// `Some(false)` quando o SUAP mostra "Inativo".
    pub active: Option<bool>,
    pub needs_correction: bool,
    pub pending_review: bool,
    /// Texto da data como o SUAP a mostra; só serve para saber se mudou.
    pub updated_at: Option<String>,
    pub updated_by: Option<String>,
    pub services: Vec<String>,
    pub groups: Vec<String>,
    pub attachments: Vec<AttachmentRef>,
    /// Endereços das imagens usadas no Resumo e na Solução (sem `data:`).
    pub images: Vec<String>,
    pub summary: String,
    pub solution: String,
}

impl Entry {
    /// Resumo estável do conteúdo (FNV-1a de 64 bits): o `atualizado_em` do SUAP só é gravado na
    /// criação da entrada, então a mudança de conteúdo é que diz se é preciso baixar de novo.
    pub fn content_hash(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        });
        format!("{hash:016x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_round_trips_names() {
        for visibility in [
            Visibility::Public,
            Visibility::Private,
            Visibility::Confidential,
        ] {
            assert_eq!(Visibility::parse(visibility.as_str()), Some(visibility));
        }
        assert_eq!(Visibility::parse(" Pública "), Some(Visibility::Public));
        assert_eq!(Visibility::parse("outra"), None);
    }

    #[test]
    fn content_hash_follows_the_content() {
        let mut entry = Entry {
            id: 1,
            title: "t".into(),
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
            images: vec![],
            summary: "a".into(),
            solution: String::new(),
        };
        let before = entry.content_hash();
        assert_eq!(before.len(), 16);
        assert_eq!(before, entry.clone().content_hash());
        entry.summary = "b".into();
        assert_ne!(before, entry.content_hash());
    }
}
