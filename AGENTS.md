# AGENTS.md

Guia para agentes (e pessoas) que trabalham neste repositório: `suap-companion/baseconhecimento`, biblioteca Rust e CLI que espelham a Base de Conhecimento do SUAP para uso offline. Fale e escreva em **português do Brasil**.

## Mapa

- `crates/baseconhecimento-core`: `parse` (HTML do SUAP), `markdown` (HTML → Markdown), `sanitize` (nomes de arquivo), `store` (pasta local, frontmatter, manifesto), `index` (busca sobre o `suap-rag`), `crawl` (atualização incremental), `source` (SUAP autenticado atrás do trait `Fetcher`), `settings`, `schedule`, `clock`.
- `crates/baseconhecimento-cli`: a CLI (`lib.rs` tem a lógica; `main.rs` é invólucro fino excluído da cobertura). Testes em `tests/cli.rs` contra um SUAP falso (wiremock).
- **Núcleo compartilhado**: `suap-core` e `suap-rag` vêm do repo [`suap-companion/suap-kit`](https://github.com/suap-companion/suap-kit) por dependência Git fixada em tag (`[workspace.dependencies]` do `Cargo.toml` da raiz). Não os copie para cá: mude o kit (PR, CI, tag) e atualize a tag.
- `site/`: central de ajuda (GitHub Pages, `pages.yml`). `docs/`: documentação técnica.
- O HTML real do SUAP local está em `../suap/centralservicos` (`urls.py`, `models.py` `BaseConhecimento`/`BaseConhecimentoAnexo`, templates `baseconhecimento.html`, `visualizar_baseconhecimento*.html`, `listar_area_servico.html`): consulte antes de mexer em um parser.

## Validação (antes de entregar)

- Sem Rust local: `sh scripts/validar-docker.sh` (constrói a imagem `baseconhecimento-validar` a partir de `rust:latest`). Com Rust: `sh scripts/validar.sh`. Roda `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo llvm-cov ... --ignore-filename-regex 'main\.rs' --fail-under-lines 100`.
- Cobertura de **100% das linhas** é regra (RS-01): uma linha só com `)?;` conta como perdida (um `writeln!(...)?;` que o rustfmt quebra em várias linhas; use `say(out, &texto)?` com o texto montado antes). Código por `cfg!(windows)` vira uma função com parâmetro testável nos dois ramos.
- Mudança em workflow do GitHub Actions: carregue a skill `github-actions` e consulte as versões das actions com `gh api repos/<dono>/<action>/releases/latest`.

## Regras do projeto

- **Privacidade**: entradas privadas/sigilosas ficam só na máquina. Nada de conteúdo real da Base de Conhecimento, senha, cookie ou chave em código, testes, logs, issues ou PRs: fixtures **sintéticas**. O progresso da CLI mostra ids e contagens, nunca títulos nem textos.
- **Rede**: só o SUAP do perfil; anexos de outro host nunca são baixados (a sessão não vai a terceiros).
- **Dependências novas** só com justificativa.
- **Git**: branch `issues/<número>`; commits em pt-BR no padrão `tipo(escopo): [TAG] Descrição`, com **Motivo**, **Impacto** e **Arquivos** no corpo; autoria de Kelson da Costa Medeiros, sem trailer `Co-Authored-By`.
- **Release**: aumente a versão em `Cargo.toml` (`[workspace.package]`), faça o PR, espere o CI, mescle e crie a release (`gh release create vX.Y.Z`: o workflow Release só dispara em `release: published`); espere o workflow **terminar com sucesso (5 assets: 4 binários e `SHA256SUMS.txt`)** e só então feche a issue. Registre as horas (`Estimated (h)`/`Spent (h)`) nas issues.
- Scripts Python com acentos: grave o `.py` num arquivo e rode com `python -I`.

## Limitações conhecidas

- As páginas de área (`/centralservicos/baseconhecimento/dashboard/<slug>/`) só listam entradas com serviço vinculado. `atualizar --varrer` tenta também os ids que faltam (buracos e ids acima do maior, parando após `--parada` negativas seguidas); os ids negados ficam em `varredura.json` e não são tentados de novo sem `--forcar`. `atualizar --id N` baixa uma entrada específica.
- O `atualizado_em` do SUAP não muda nas edições; a detecção de mudança usa um resumo do conteúdo (exige buscar a página de cada entrada).
- A busca é por palavras (FTS5/BM25), sem sinônimos; PDFs digitalizados (sem texto) não são indexados.
