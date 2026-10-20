# Arquitetura

## Rastreamento (`crawl`, `parse`)

Rotas do módulo `centralservicos` do SUAP, com a sessão do perfil:

1. `GET /centralservicos/baseconhecimento/`: lista as áreas (`a.box-item` com link para `dashboard/<slug>/`).
2. `GET /centralservicos/baseconhecimento/dashboard/<slug>/`: os links `/centralservicos/baseconhecimento/<id>/` de cada área (inclusive os escondidos em "Mostrar mais"). Área negada (403/404) é contada e pulada; uma listagem incompleta nunca poda a cópia local.
3. `GET /centralservicos/baseconhecimento/<id>/`: título, área (`dl.details-container`), tags (`ul.tags`), situação (`span.status`: Ativo/Inativo, Pendente de correção, Supervisão pendente), visibilidade (`status-publica|privada|sigilosa`), Resumo e Solução (`#resumo`/`#solucao`), serviços, grupos de atendimento e anexos (abas, visíveis para atendentes) e "Última Atualização por ... em ...".
4. Imagens do Resumo e da Solução: baixadas (mesmo host) para `anexos/<id>/imagens/` e os links do Markdown reescritos para o caminho local.
5. Anexos: o `href` de cada link, baixado só se for do mesmo host do SUAP, com o nome sanitizado (sem pastas, sem caracteres proibidos no Windows, sem nomes reservados, tamanho limitado, sem colisão).

O que a pessoa logada não enxerga (403/404) vira "sem acesso" no resumo. A sessão expirada interrompe a rodada com o pedido de novo login.

## Atualização incremental

`atualizar` descobre os ids pelas áreas (ou usa `--id`), busca a página de cada entrada e compara o **resumo do conteúdo** (FNV-1a de 64 bits do JSON da entrada) com o do manifesto. O `atualizado_em` do SUAP é gravado só na criação da entrada (`auto_now_add`), por isso não serve para detectar edições. Mudou, ou `--forcar`: baixa os anexos, reescreve o Markdown, reindexa e grava o manifesto (entrada a entrada, então uma rodada interrompida não perde o que já veio). `--podar` apaga o que sumiu da listagem, mas só quando todas as áreas foram listadas.

## Conversão HTML → Markdown (`markdown`)

Títulos, parágrafos, ênfases, código, listas aninhadas, links, imagens, citações, blocos de código, tabelas (GFM) e linhas horizontais. Marcas desconhecidas viram o texto; `script`/`style` são descartados. O Markdown de cada entrada tem frontmatter YAML (`id`, `titulo`, `url`, `area`, `visibilidade`, `atualizado_em`, `atualizado_por`, `ativo`, `necessita_correcao`, `supervisao_pendente`, `tags`, `servicos`, `grupos`, `anexos`, `baixado_em`) e as seções Resumo, Solução e Anexos.

## Índice de busca (`index`)

O `suap-rag` guarda os trechos (SQLite/FTS5, BM25, sem acentos nem caixa). Cada arquivo é um documento chamado `<id>-<nome>`: o Markdown da entrada e os anexos TXT, MD e PDF (PDF por página). A busca devolve o trecho, a entrada de origem, a visibilidade e o anexo/página.

## Perfis e pastas

Os perfis ficam no `~/.config/suap/config.toml` do `suap-core` (o mesmo do `chamados`). Dados em `<dados>/base/<perfil>/`; a sessão em `session[-perfil].cookies` do diretório de dados do `baseconhecimento`.

## App Android

A fachada `baseconhecimento-mobile` (UniFFI) expõe a biblioteca ao app Kotlin em `android/`: sessão por cookies da WebView, leitura offline (`entries`, `entry`, `search`), `Settings` e `crawl::update` com progresso consultável e cancelamento (`UpdateOptions::cancel`). Veja [android.md](android.md).

## Limitações

- Entradas que nenhuma área lista (sem serviço vinculado) chegam por `atualizar --varrer` (tenta os ids que faltam e para após `--parada` negativas seguidas; os negados ficam em `varredura.json`) ou por `atualizar --id N`. Com `--varrer`, `--podar` também apaga a entrada que o SUAP deixou de mostrar.
- Comentários internos das entradas não são baixados (não fazem parte do artigo).
- A imagem do texto que não puder ser baixada continua apontando para o SUAP; imagens `data:` ficam como estão.
- PDFs sem texto (digitalizados) ou protegidos não entram no índice.
