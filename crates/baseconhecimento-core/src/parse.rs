//! Leitura das páginas da Base de Conhecimento da Central de Serviços do SUAP.
//!
//! O markup vem dos templates `listar_area_servico.html`, `baseconhecimento.html` e
//! `visualizar_baseconhecimento.html` do módulo `centralservicos`.

use scraper::{ElementRef, Html, Selector};

use crate::{
    markdown,
    model::{AttachmentRef, Entry, Visibility},
    sanitize,
};

/// Caminho da lista de áreas.
pub const AREAS_PATH: &str = "/centralservicos/baseconhecimento/";
const DASHBOARD_PREFIX: &str = "/centralservicos/baseconhecimento/dashboard/";

/// Uma área de atuação com artigos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    pub slug: String,
    pub name: String,
}

/// Caminho da página de uma área.
pub fn dashboard_path(slug: &str) -> String {
    format!("{DASHBOARD_PREFIX}{slug}/")
}

/// Caminho da página de uma entrada.
pub fn entry_path(id: u64) -> String {
    format!("{AREAS_PATH}{id}/")
}

/// Por que a página não pôde ser lida como entrada.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error(
        "a página da entrada {0} não tem título: não parece uma entrada da Base de Conhecimento"
    )]
    NoTitle(u64),
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("seletor estático válido")
}

fn text_of(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// As áreas listadas em `/centralservicos/baseconhecimento/`.
pub fn parse_areas(html: &str) -> Vec<Area> {
    let document = Html::parse_document(html);
    let title = selector("h4");
    let mut areas: Vec<Area> = Vec::new();
    for link in document.select(&selector("a.box-item[href]")) {
        let href = link.value().attr("href").unwrap_or_default();
        let Some(rest) = href.strip_prefix(DASHBOARD_PREFIX) else {
            continue;
        };
        let slug = rest.trim_matches('/').to_owned();
        if slug.is_empty() || slug.contains('/') || areas.iter().any(|a| a.slug == slug) {
            continue;
        }
        let name = link.select(&title).next().map(text_of).unwrap_or_default();
        areas.push(Area { slug, name });
    }
    areas
}

/// Os ids das entradas ligadas numa página (dashboard de área): na ordem, sem repetir.
pub fn parse_entry_ids(html: &str) -> Vec<u64> {
    let document = Html::parse_document(html);
    let mut ids: Vec<u64> = Vec::new();
    for link in document.select(&selector("a[href]")) {
        let href = link.value().attr("href").unwrap_or_default();
        let id = href
            .strip_prefix(AREAS_PATH)
            .map(|rest| rest.trim_matches('/'))
            .and_then(|rest| rest.parse::<u64>().ok());
        if let Some(id) = id.filter(|id| !ids.contains(id)) {
            ids.push(id);
        }
    }
    ids
}

/// O corpo (`.accordion-body`) do box com esse id (o `box` do SUAP usa o título como id).
fn box_html(document: &Html, id: &str) -> String {
    let css = format!("[id=\"{id}\"] .accordion-body");
    document
        .select(&selector(&css))
        .next()
        .map(|body| body.inner_html())
        .unwrap_or_default()
}

fn list_items(document: &Html, tab: &str) -> Vec<String> {
    let css = format!("div.tab-pane[data-title=\"{tab}\"] li");
    document.select(&selector(&css)).map(text_of).collect()
}

/// O texto da `dd` logo depois da `dt` com esse rótulo ("Área:").
fn definition(document: &Html, label: &str) -> Option<String> {
    let term = document
        .select(&selector("dl.details-container dt"))
        .find(|term| text_of(*term) == label)?;
    let description = term.next_siblings().filter_map(ElementRef::wrap).next()?;
    Some(text_of(description)).filter(|text| !text.is_empty())
}

/// "Última Atualização por Fulano em 01/01/2026 10:00": quem e quando.
fn last_update(document: &Html) -> (Option<String>, Option<String>) {
    let line = document
        .select(&selector("li"))
        .map(text_of)
        .find(|text| text.starts_with("Última Atualização"));
    let Some(line) = line else {
        return (None, None);
    };
    let rest = line.trim_start_matches("Última Atualização").trim();
    let (by, when) = match rest.strip_prefix("por ") {
        Some(after) => match after.split_once(" em ") {
            Some((by, when)) => (Some(by.trim().to_owned()), when),
            None => (Some(after.to_owned()), ""),
        },
        None => (None, rest.strip_prefix("em ").unwrap_or_default()),
    };
    (by, Some(when.trim().to_owned()).filter(|w| !w.is_empty()))
}

/// Os `src` das imagens de um trecho de HTML: na ordem, sem repetir e sem `data:`.
fn image_urls(html: &str) -> Vec<String> {
    let fragment = Html::parse_fragment(html);
    let mut urls: Vec<String> = Vec::new();
    for image in fragment.select(&selector("img[src]")) {
        let src = image.value().attr("src").unwrap_or_default().trim();
        if !src.is_empty() && !src.starts_with("data:") && !urls.iter().any(|known| known == src) {
            urls.push(src.to_owned());
        }
    }
    urls
}

fn attachments(document: &Html) -> Vec<AttachmentRef> {
    let css = "div.tab-pane[data-title=\"Anexos\"] a[href], [id=\"anexos\"] a[href]";
    let mut found: Vec<AttachmentRef> = Vec::new();
    for link in document.select(&selector(css)) {
        let url = link.value().attr("href").unwrap_or_default().to_owned();
        if found.iter().any(|known| known.url == url) {
            continue;
        }
        let shown = text_of(link);
        let name = if shown.is_empty() {
            sanitize::name_from_url(&url)
        } else {
            sanitize::file_name(&shown)
        };
        found.push(AttachmentRef { url, name });
    }
    found
}

/// Lê a página de uma entrada (`/centralservicos/baseconhecimento/<id>/`).
pub fn parse_entry(id: u64, html: &str) -> Result<Entry, ParseError> {
    let document = Html::parse_document(html);
    let title = ["h2:not(.accordion-header)", "h1", "title"]
        .iter()
        .find_map(|css| {
            document
                .select(&selector(css))
                .map(text_of)
                .find(|t| !t.is_empty())
        })
        .ok_or(ParseError::NoTitle(id))?;
    let status = |css: &str| {
        document
            .select(&selector(css))
            .map(text_of)
            .collect::<Vec<_>>()
    };
    let flags = status("span.status");
    let visibility = ["publica", "privada", "sigilosa"]
        .iter()
        .find(|name| {
            document
                .select(&selector(&format!("span.status-{name}")))
                .next()
                .is_some()
        })
        .and_then(|name| Visibility::parse(name));
    let active = if flags.iter().any(|f| f == "Inativo") {
        Some(false)
    } else {
        flags.iter().any(|f| f == "Ativo").then_some(true)
    };
    let (updated_by, updated_at) = last_update(&document);
    let (summary, solution) = (
        box_html(&document, "resumo"),
        box_html(&document, "solucao"),
    );
    let images = image_urls(&format!("{summary}{solution}"));
    Ok(Entry {
        id,
        title,
        area: definition(&document, "Área:"),
        visibility,
        tags: document
            .select(&selector("ul.tags li"))
            .map(text_of)
            .collect(),
        active,
        needs_correction: flags.iter().any(|f| f == "Pendente de correção"),
        pending_review: flags.iter().any(|f| f == "Supervisão pendente"),
        updated_at,
        updated_by,
        services: list_items(&document, "Serviços vinculados"),
        groups: list_items(&document, "Grupos de atendimento"),
        attachments: attachments(&document),
        images,
        summary: markdown::from_html(&summary),
        solution: markdown::from_html(&solution),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const AREAS: &str = r#"<div class="boxes links flex-container">
        <a href="/centralservicos/baseconhecimento/dashboard/ti/" class="box-item"><h4>Tecnologia da Informação</h4></a>
        <a href="/centralservicos/baseconhecimento/dashboard/ti/" class="box-item"><h4>Repetida</h4></a>
        <a href="/centralservicos/baseconhecimento/dashboard/" class="box-item"><h4>Sem slug</h4></a>
        <a href="/centralservicos/baseconhecimento/dashboard/a/b/" class="box-item"><h4>Fundo</h4></a>
        <a href="/outra/" class="box-item"><h4>Fora</h4></a>
        <a href="/centralservicos/baseconhecimento/dashboard/rh/" class="box-item"></a></div>"#;

    pub const ENTRY: &str = r##"<html><head><title>Ignorado</title></head><body>
        <h2>Como configurar o <i>Wi-Fi</i></h2>
        <span class="status status-success">Ativo</span>
        <span class="status status-alert">Pendente de correção</span>
        <span class="status status-alert">Supervisão pendente</span>
        <span class="status status-privada">Privada</span>
        <div class="accordion"><h2 class="accordion-header"><button>Resumo</button></h2>
          <div id="resumo" class="accordion-collapse"><div class="accordion-body"><p>Passo a passo&nbsp;do <b>Wi-Fi</b><br>da sala.</p></div></div></div>
        <div class="accordion"><div id="solucao"><div class="accordion-body"><div class="richtext"><ol><li>Abra</li><li>Conecte</li></ol></div></div></div></div>
        <div class="tab-pane" data-title="Serviços vinculados"><ul><li>Rede sem fio</li><li>Internet</li></ul></div>
        <div class="tab-pane" data-title="Grupos de atendimento"><ul><li>Suporte  N1</li></ul></div>
        <div class="tab-pane" data-title="Anexos"><table><tr><td><a href="/media/upload/manual%20wifi.pdf" target="_blank">manual wifi.pdf</a></td></tr>
          <tr><td><a href="/media/upload/b.png" target="_blank"> </a></td></tr>
          <tr><td><a href="/media/upload/b.png" target="_blank">repetido.png</a></td></tr></table></div>
        <aside><dl class="details-container"><dt>Área:</dt><dd>Tecnologia da Informação</dd>
          <dt>Tags:</dt><dd><ul class="tags"><li>Rede</li><li>Wi-Fi</li></ul></dd></dl></aside>
        <ul><li>Outra coisa</li><li>Última Atualização por Maria Souza em 02/03/2026 10:15</li></ul></body></html>"##;

    #[test]
    fn lists_areas_once_each() {
        let areas = parse_areas(AREAS);
        assert_eq!(
            areas,
            vec![
                Area {
                    slug: "ti".into(),
                    name: "Tecnologia da Informação".into()
                },
                Area {
                    slug: "rh".into(),
                    name: String::new()
                },
            ]
        );
    }

    #[test]
    fn builds_paths() {
        assert_eq!(
            dashboard_path("ti"),
            "/centralservicos/baseconhecimento/dashboard/ti/"
        );
        assert_eq!(entry_path(7), "/centralservicos/baseconhecimento/7/");
    }

    #[test]
    fn lists_entry_ids_in_order_without_repeats() {
        let html = r#"<a href="/centralservicos/baseconhecimento/12/">a</a>
            <a href="/centralservicos/baseconhecimento/5/">b</a><a href="/centralservicos/baseconhecimento/12/">c</a>
            <a href="/centralservicos/baseconhecimento/dashboard/ti/">d</a><a href="/x/9/">e</a><a>f</a>"#;
        assert_eq!(parse_entry_ids(html), vec![12, 5]);
    }

    #[test]
    fn reads_every_field_of_an_entry() {
        let entry = parse_entry(42, ENTRY).unwrap();
        assert_eq!(entry.id, 42);
        assert_eq!(entry.title, "Como configurar o Wi-Fi");
        assert_eq!(entry.area.as_deref(), Some("Tecnologia da Informação"));
        assert_eq!(entry.visibility, Some(Visibility::Private));
        assert_eq!(entry.tags, vec!["Rede", "Wi-Fi"]);
        assert_eq!(entry.active, Some(true));
        assert!(entry.needs_correction && entry.pending_review);
        assert_eq!(entry.updated_at.as_deref(), Some("02/03/2026 10:15"));
        assert_eq!(entry.updated_by.as_deref(), Some("Maria Souza"));
        assert_eq!(entry.services, vec!["Rede sem fio", "Internet"]);
        assert_eq!(entry.groups, vec!["Suporte N1"]);
        assert_eq!(entry.summary, "Passo a passo do **Wi-Fi**  \nda sala.");
        assert_eq!(entry.solution, "1. Abra\n2. Conecte");
        let names: Vec<_> = entry.attachments.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["manual wifi.pdf", "b.png"]);
    }

    #[test]
    fn lists_embedded_images_once() {
        let html = r#"<h2>T</h2><div id="resumo"><div class="accordion-body"><img src="/media/a.png"><img src=" /media/a.png "><img src="data:image/png;base64,AA"><img src=""></div></div>
            <div id="solucao"><div class="accordion-body"><img src="/media/b.jpg"></div></div>"#;
        assert_eq!(
            parse_entry(1, html).unwrap().images,
            vec!["/media/a.png", "/media/b.jpg"]
        );
    }

    #[test]
    fn reads_a_minimal_entry() {
        let entry = parse_entry(1, "<title>Só título</title><span class=\"status status-error\">Inativo</span><span class=\"status status-sigilosa\">Sigilosa</span>").unwrap();
        assert_eq!(entry.title, "Só título");
        assert_eq!(entry.active, Some(false));
        assert_eq!(entry.visibility, Some(Visibility::Confidential));
        assert_eq!(
            (entry.area, entry.updated_at, entry.updated_by),
            (None, None, None)
        );
        assert!(entry.summary.is_empty() && entry.attachments.is_empty());
        let plain = parse_entry(2, "<h1>Sem estado</h1>").unwrap();
        assert_eq!((plain.active, plain.visibility), (None, None));
    }

    #[test]
    fn reads_the_public_attachments_box() {
        let html = r#"<h2>T</h2><div id="anexos"><ul><li><a href="/media/x/Guia.PDF" target="_blank"></a></li></ul></div>"#;
        let entry = parse_entry(3, html).unwrap();
        assert_eq!(entry.attachments[0].name, "Guia.PDF");
    }

    #[test]
    fn reads_partial_update_lines() {
        let by_only =
            parse_entry(1, "<h2>T</h2><ul><li>Última Atualização por Ana</li></ul>").unwrap();
        assert_eq!(
            (by_only.updated_by.as_deref(), by_only.updated_at),
            (Some("Ana"), None)
        );
        let when_only = parse_entry(
            1,
            "<h2>T</h2><ul><li>Última Atualização em 01/01/2026</li></ul>",
        )
        .unwrap();
        assert_eq!(
            (when_only.updated_by, when_only.updated_at.as_deref()),
            (None, Some("01/01/2026"))
        );
        let bare = parse_entry(1, "<h2>T</h2><ul><li>Última Atualização</li></ul>").unwrap();
        assert_eq!((bare.updated_by, bare.updated_at), (None, None));
    }

    #[test]
    fn rejects_pages_without_title() {
        let error = parse_entry(9, "<p>nada</p>").unwrap_err();
        assert!(error.to_string().contains("entrada 9"));
    }

    #[test]
    fn empty_definition_is_none() {
        let html = r#"<h2>T</h2><dl class="details-container"><dt>Área:</dt><dd> </dd></dl><dl class="details-container"><dt>Tags:</dt></dl>"#;
        assert_eq!(parse_entry(1, html).unwrap().area, None);
    }
}
