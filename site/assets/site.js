// Central de ajuda do baseconhecimento: links de download e versão (release mais recente) e busca.
// O HTML funciona sem este script: os links apontam para a página da release.
(function () {
  "use strict";
  var API = "https://api.github.com/repos/suap-companion/baseconhecimento/releases";
  var assets = {
    "windows": "-windows-x86_64.zip",
    "linux": "-linux-x86_64.tar.gz",
    "linux-arm": "-linux-arm64.tar.gz",
    "macos": "-macos-aarch64.tar.gz",
    "android": "-android.apk"
  };

  function json(url) {
    return fetch(url, { headers: { Accept: "application/vnd.github+json" } })
      .then(function (r) { return r.ok ? r.json() : Promise.reject(new Error("http " + r.status)); });
  }

  // ---- versão e downloads
  var versions = document.querySelectorAll("[data-version]");
  var links = document.querySelectorAll("[data-asset]");
  if (versions.length || links.length) {
    json(API + "/latest").then(function (release) {
      versions.forEach(function (el) { el.textContent = release.tag_name; });
      links.forEach(function (link) {
        var suffix = assets[link.getAttribute("data-asset")];
        (release.assets || []).forEach(function (asset) {
          if (suffix && asset.name.endsWith(suffix)) { link.href = asset.browser_download_url; }
        });
      });
    }).catch(function () {
      versions.forEach(function (el) { el.textContent = "mais recente"; });
    });
  }

  // ---- busca
  var index = [
    ["Instalar", "instalar.html", "baixar instalar windows linux macos path zip tar checksum"],
    ["Primeiro uso e login", "instalar.html#primeiro-uso", "profile init login senha suap_password perfil chamados config.toml"],
    ["Baixar e atualizar", "usar.html#atualizar", "atualizar baixar incremental forcar id podar anexos"],
    ["Listar e ler entradas", "usar.html#listar", "listar mostrar markdown area visibilidade arquivos editor obsidian"],
    ["Buscar", "usar.html#buscar", "buscar busca palavras pdf anexos json limite"],
    ["Status e configurações", "usar.html#status", "status config intervalo dias reindexar"],
    ["Perfis", "usar.html#perfis", "perfil homologacao suap ambientes sessão"],
    ["App Android", "android.html", "android celular apk instalar app offline"],
    ["Entrar no app Android", "android.html#entrar", "android login webview sessão senha gov.br"],
    ["Baixar a base no celular", "android.html#primeira", "android primeira atualização wifi cancelar andamento"],
    ["Ler, buscar e filtrar no app", "android.html#ler", "android lista filtros busca leitura anexos tags área visibilidade"],
    ["Atualização automática no app", "android.html#automatica", "android automática agenda intervalo wifi bateria notificação"],
    ["Atualização semanal", "atualizacao.html", "semanal agendar agendador cron systemd windows tarefa se-vencido"],
    ["Ajustar a atualização automática", "atualizacao.html#ajustar", "config auto-atualizar intervalo pausa status"],
    ["Sessão expirada", "faq.html#sessao", "sessão expirada login entrar de novo"],
    ["Faltam entradas", "faq.html#faltam", "faltam entradas sem acesso 403 404 área negada id"],
    ["A busca não acha o anexo", "faq.html#busca", "anexo pdf digitalizado ocr protegido reindexar"],
    ["App: a atualização não roda sozinha", "faq.html#android-sessao", "android atualização automática não roda bateria wifi sessão"],
    ["App: imagem ou link offline", "faq.html#android-offline", "android imagem anexo link offline"],
    ["Privacidade e segurança", "privacidade.html", "privacidade privada sigilosa senha dados segurança local"]
  ];
  var input = document.getElementById("busca");
  var results = document.getElementById("resultados");
  function norm(text) { return text.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, ""); }
  if (input && results) {
    input.addEventListener("input", function () {
      var terms = norm(input.value).split(/\s+/).filter(Boolean);
      results.textContent = "";
      if (!terms.length) { results.hidden = true; return; }
      var found = index.filter(function (entry) {
        var haystack = norm(entry[0] + " " + entry[2]);
        return terms.every(function (term) { return haystack.indexOf(term) >= 0; });
      }).slice(0, 6);
      if (!found.length) {
        var none = document.createElement("li");
        none.textContent = "Nada encontrado. Tente outras palavras.";
        results.appendChild(none);
      }
      found.forEach(function (entry) {
        var item = document.createElement("li");
        var link = document.createElement("a");
        link.href = entry[1];
        link.textContent = entry[0];
        item.appendChild(link);
        results.appendChild(item);
      });
      results.hidden = false;
    });
    document.addEventListener("click", function (event) {
      if (!results.contains(event.target) && event.target !== input) { results.hidden = true; }
    });
    input.addEventListener("keydown", function (event) { if (event.key === "Escape") { results.hidden = true; } });
  }
})();
