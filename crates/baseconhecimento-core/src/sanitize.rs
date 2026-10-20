//! Nomes de arquivo seguros para os anexos baixados do SUAP.

const MAX_LEN: usize = 100;
const RESERVED: [&str; 8] = ["con", "prn", "aux", "nul", "com1", "com2", "lpt1", "lpt2"];

/// Nome seguro para gravar em disco: sem pastas, sem caracteres de controle ou proibidos no
/// Windows, sem ponto nas pontas, sem nome reservado e com tamanho limitado.
pub fn file_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    let mut name: String = trimmed.chars().take(MAX_LEN).collect();
    let stem = name.split('.').next().unwrap_or_default().to_lowercase();
    if name.is_empty() || RESERVED.contains(&stem.as_str()) {
        name = format!("anexo_{name}");
    }
    name
}

/// Nome do anexo a partir do endereço: a última parte do caminho, sem a consulta, decodificada.
pub fn name_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let last = path.rsplit('/').next().unwrap_or_default();
    file_name(&percent_decode(last))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[index], hex) {
            (b'%', Some(value)) => {
                out.push(value);
                index += 3;
            }
            (byte, _) => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Acrescenta `-2`, `-3`... antes da extensão até o nome não colidir com `taken`.
pub fn unique(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|other| other == name) {
        return name.to_owned();
    }
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    let mut n = 2;
    loop {
        let candidate = format!("{stem}-{n}{extension}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Trecho curto e seguro, para nomes de arquivo, a partir de um título.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().map(strip_accent) {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(60).collect()
}

fn strip_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_removes_paths_and_dangerous_characters() {
        assert_eq!(file_name("../../etc/passwd"), "passwd");
        assert_eq!(file_name("C:\\dir\\a:b?.pdf"), "a_b_.pdf");
        assert_eq!(file_name("...oculto."), "oculto");
        assert_eq!(file_name("con.txt"), "anexo_con.txt");
        assert_eq!(file_name(""), "anexo_");
        assert_eq!(file_name("a\u{0}b"), "a_b");
        assert_eq!(file_name(&"x".repeat(300)).len(), MAX_LEN);
    }

    #[test]
    fn name_from_url_decodes_and_drops_query() {
        assert_eq!(
            name_from_url("/media/a/Manual%20do%20Usu%C3%A1rio.pdf?x=1#y"),
            "Manual do Usuário.pdf"
        );
        assert_eq!(name_from_url("/media/50%.txt"), "50%.txt");
        assert_eq!(name_from_url("/media/%zz.txt"), "%zz.txt");
    }

    #[test]
    fn unique_adds_a_counter() {
        let taken = vec!["a.pdf".to_owned(), "a-2.pdf".to_owned(), "b".to_owned()];
        assert_eq!(unique("c.pdf", &taken), "c.pdf");
        assert_eq!(unique("a.pdf", &taken), "a-3.pdf");
        assert_eq!(unique("b", &taken), "b-2");
        assert_eq!(unique(".x", &[".x".to_owned()]), ".x-2");
    }

    #[test]
    fn slug_folds_accents() {
        assert_eq!(
            slug("Configuração do Wi-Fi: Á é í ó ú"),
            "configuracao-do-wi-fi-a-e-i-o-u"
        );
        assert_eq!(slug("***"), "");
        assert_eq!(slug(&"a".repeat(100)).len(), 60);
    }
}
