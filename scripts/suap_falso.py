"""SUAP falso, com dados sintéticos, para testar a atualização do app Android sem o SUAP real.

Uso: python -I scripts/suap_falso.py [porta]   (padrão 8765)

O emulador enxerga o computador em http://10.0.2.2:<porta>/ (o app libera HTTP sem TLS só para esse endereço).
Serve o login, uma área, três entradas (uma privada com anexo e imagem, uma pública e uma negada com 403) e os
arquivos. Nada aqui vem do SUAP real.
"""

import base64
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BASE = "/centralservicos/baseconhecimento"
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
)

PAGES = {
    "/": "<html>inicio</html>",
    "/accounts/login/": '<form><input type="hidden" name="csrfmiddlewaretoken" value="tok"></form>',
    f"{BASE}/": f'<a class="box-item" href="{BASE}/dashboard/ti/"><h4>Tecnologia</h4></a>',
    f"{BASE}/dashboard/ti/": f'<a href="{BASE}/1/">a</a><a href="{BASE}/2/">b</a><a href="{BASE}/3/">c</a>',
    f"{BASE}/1/": """<h2>Trocar o toner</h2><span class="status status-privada">Privada</span>
        <div id="resumo"><div class="accordion-body"><p>Procedimento para a impressora do setor.</p>
        <img src="/media/toner.png" alt="toner"></div></div>
        <div class="tab-pane" data-title="Anexos"><a href="/media/guia.txt">guia.txt</a></div>
        <dl class="details-container"><dt>Área:</dt><dd>Tecnologia</dd></dl>
        <ul class="tags"><li>impressora</li><li>toner</li></ul>
        <ul><li>Última Atualização por Pessoa em 01/01/2026 10:00</li></ul>""",
    f"{BASE}/2/": """<h2>Solicitar conta</h2><span class="status status-publica">Pública</span>
        <div id="solucao"><div class="accordion-body"><p>Abra um chamado de acesso.</p></div></div>""",
}
FILES = {
    "/media/toner.png": ("image/png", PNG),
    "/media/guia.txt": ("text/plain; charset=utf-8", "trocar o cartucho do toner".encode()),
}


class Handler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        path = self.path.split("?")[0]
        if path in PAGES:
            self.send("text/html; charset=utf-8", PAGES[path].encode())
        elif path in FILES:
            self.send(*FILES[path])
        else:
            self.send("text/plain", b"negado", 403 if path.startswith(BASE) else 404)

    def send(self, kind: str, body: bytes, status: int = 200) -> None:
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_: object) -> None:
        pass


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    print(f"SUAP falso em http://0.0.0.0:{port}/ (no emulador: http://10.0.2.2:{port}/)", flush=True)
    ThreadingHTTPServer(("0.0.0.0", port), Handler).serve_forever()
