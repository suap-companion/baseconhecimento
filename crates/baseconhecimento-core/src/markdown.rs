//! Conversão de HTML (o texto rico do SUAP) para Markdown.
//!
//! Cobre o que o editor do SUAP produz: títulos, parágrafos, ênfases, listas aninhadas, links,
//! imagens, citações, código, tabelas e linhas horizontais. Marcas desconhecidas viram só o texto.

use scraper::{ElementRef, Html};

/// Converte um trecho de HTML em Markdown, sem espaços nem linhas em branco sobrando.
pub fn from_html(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    tidy(&children(fragment.root_element()))
}

fn children(element: ElementRef<'_>) -> String {
    let mut out = String::new();
    for child in element.children() {
        if let Some(inner) = ElementRef::wrap(child) {
            out.push_str(&render(inner));
        } else if let Some(text) = child.value().as_text() {
            out.push_str(&collapse(text));
        }
    }
    out
}

/// Sequências de espaços, tabulações e quebras de linha viram um espaço só (como no navegador).
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_whitespace() {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn block(inner: &str) -> String {
    format!("\n\n{}\n\n", inner.trim())
}

fn wrap(inner: &str, mark: &str) -> String {
    let text = inner.trim();
    if text.is_empty() {
        return inner.to_owned();
    }
    let lead = &inner[..inner.len() - inner.trim_start().len()];
    let trail = &inner[inner.trim_end().len()..];
    format!("{lead}{mark}{text}{mark}{trail}")
}

fn render(element: ElementRef<'_>) -> String {
    let name = element.value().name();
    let inner = || children(element);
    match name {
        "script" | "style" | "head" | "noscript" | "template" => String::new(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = usize::from(name.as_bytes()[1] - b'0');
            block(&format!("{} {}", "#".repeat(level), inner().trim()))
        }
        "p" | "div" | "section" | "article" | "header" | "footer" | "main" | "aside" | "figure"
        | "figcaption" | "details" | "summary" | "dl" | "dt" | "dd" => block(&inner()),
        "br" => "  \n".to_owned(),
        "hr" => block("---"),
        "strong" | "b" => wrap(&inner(), "**"),
        "em" | "i" => wrap(&inner(), "*"),
        "del" | "s" | "strike" => wrap(&inner(), "~~"),
        "code" => wrap(&inner(), "`"),
        "a" => link(element, &inner()),
        "img" => image(element),
        "pre" => block(&format!(
            "```\n{}\n```",
            raw_text(element).trim_matches('\n')
        )),
        "blockquote" => block(&quote(&inner())),
        "ul" | "ol" => block(&list(element, name == "ol")),
        "table" => block(&table(element)),
        _ => inner(),
    }
}

fn link(element: ElementRef<'_>, inner: &str) -> String {
    let text = inner.trim();
    match element.value().attr("href") {
        Some(href) if !href.is_empty() && !text.is_empty() => format!("[{text}]({href})"),
        _ => inner.to_owned(),
    }
}

fn image(element: ElementRef<'_>) -> String {
    let alt = element.value().attr("alt").unwrap_or_default();
    match element.value().attr("src") {
        Some(src) if !src.is_empty() => format!("![{alt}]({src})"),
        _ => alt.to_owned(),
    }
}

fn raw_text(element: ElementRef<'_>) -> String {
    element.text().collect()
}

fn quote(inner: &str) -> String {
    tidy(inner)
        .lines()
        .map(|line| format!("> {line}").trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

fn list(element: ElementRef<'_>, ordered: bool) -> String {
    let mut items = Vec::new();
    for item in element.children().filter_map(ElementRef::wrap) {
        if item.value().name() != "li" {
            continue;
        }
        let marker = if ordered {
            format!("{}. ", items.len() + 1)
        } else {
            "- ".to_owned()
        };
        let body = tidy(&children(item)).replace("\n\n", "\n");
        let pad = " ".repeat(marker.len());
        let indented = body.replace('\n', &format!("\n{pad}"));
        items.push(format!("{marker}{indented}").trim_end().to_owned());
    }
    items.join("\n")
}

fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn table(element: ElementRef<'_>) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in element
        .descendants()
        .filter_map(ElementRef::wrap)
        .filter(|row| row.value().name() == "tr")
    {
        let cells = row
            .children()
            .filter_map(ElementRef::wrap)
            .filter(|cell| matches!(cell.value().name(), "td" | "th"))
            .map(|cell| flat(&children(cell)).replace('|', "\\|"))
            .collect();
        rows.push(cells);
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let mut cells = row.clone();
        cells.resize(width, String::new());
        lines.push(format!("| {} |", cells.join(" | ")));
        if index == 0 {
            lines.push(format!("|{}", " --- |".repeat(width)));
        }
    }
    lines.join("\n")
}

/// Tira espaços das pontas, deixa no máximo uma linha em branco entre blocos e esvazia as linhas
/// só com espaços. O conteúdo de blocos de código é preservado.
fn tidy(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_code = false;
    for line in text.trim().lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        let blank = !in_code && line.trim().is_empty();
        if blank && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        lines.push(if blank { "" } else { line });
    }
    let mut code = false;
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let fence = line.trim_start().starts_with("```");
        out.push(if code || fence {
            line.to_owned()
        } else {
            squeeze(line)
        });
        code ^= fence;
    }
    out.join("\n").trim().to_owned()
}

/// Uma só separação entre palavras, mantendo o recuo e a quebra forçada (dois espaços no fim).
fn squeeze(line: &str) -> String {
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let hard_break = if body.ends_with("  ") { "  " } else { "" };
    format!("{indent}{}{hard_break}", flat(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_headings_paragraphs_and_inline_marks() {
        let md = from_html(
            "<h2>Título</h2><p>Um <strong>forte</strong>, <em>ênfase</em> e <code>cmd</code>\n  com <del>riscado</del>.<br>Outra linha.</p><hr><p> </p>",
        );
        assert_eq!(
            md,
            "## Título\n\nUm **forte**, *ênfase* e `cmd` com ~~riscado~~.  \nOutra linha.\n\n---"
        );
    }

    #[test]
    fn keeps_spaces_around_marks_and_skips_empty_marks() {
        assert_eq!(from_html("a<b> x </b>b<i> </i>c"), "a **x** b c");
    }

    #[test]
    fn converts_links_and_images() {
        let md = from_html(
            r#"<a href="/x">ver</a> <a href="">vazio</a> <a>sem</a> <a href="/y"> </a><img src="a.png" alt="foto"><img alt="só alt"><img src="">"#,
        );
        assert_eq!(md, "[ver](/x) vazio sem ![foto](a.png)só alt");
    }

    #[test]
    fn converts_nested_lists() {
        let md = from_html(
            "<ul><li>um<ul><li>aninhado</li></ul></li><li>dois</li><p>ignorado</p></ul><ol><li>a</li><li>b</li></ol>",
        );
        assert_eq!(md, "- um\n  - aninhado\n- dois\n\n1. a\n2. b");
    }

    #[test]
    fn converts_code_and_quotes() {
        let md = from_html(
            "<pre>linha 1\n\n\nlinha 3</pre><blockquote><p>citado</p><p>de novo</p></blockquote>",
        );
        assert_eq!(
            md,
            "```\nlinha 1\n\n\nlinha 3\n```\n\n> citado\n>\n> de novo"
        );
    }

    #[test]
    fn converts_tables() {
        let md = from_html(
            "<table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1 | 2</td></tr><tr><td><p>x</p><p>y</p></td><td>z</td></tr></tbody></table><table></table>",
        );
        assert_eq!(md, "| A | B |\n| --- | --- |\n| 1 \\| 2 | |\n| x y | z |");
    }

    #[test]
    fn drops_scripts_and_flattens_unknown_tags() {
        let md = from_html("<div><script>alert(1)</script><style>p{}</style><span>texto</span><custom>solto</custom></div>");
        assert_eq!(md, "textosolto");
        assert_eq!(from_html(""), "");
    }
}
