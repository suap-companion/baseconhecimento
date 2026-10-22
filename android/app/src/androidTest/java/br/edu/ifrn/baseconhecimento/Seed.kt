package br.edu.ifrn.baseconhecimento

import android.content.Context
import java.io.File
import uniffi.baseconhecimento_mobile.Base

/**
 * Uma cópia local sintética (nada do SUAP real) gravada no formato que o `baseconhecimento` usa, para testar a
 * leitura offline de ponta a ponta sem rede: duas entradas, uma privada com anexo e imagem, e o índice de busca.
 */
object Seed {
    const val URL = "https://suap.exemplo.br/"
    const val PRIVATE_TITLE = "Trocar o toner da impressora"
    /** Um PNG de 1x1 pixel. */
    private const val PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
    const val PUBLIC_TITLE = "Solicitar conta de acesso"

    private val manifest = """
        {
          "1": {"title": "$PRIVATE_TITLE", "area": "Tecnologia", "visibility": "privada", "tags": ["impressora", "toner"],
                "updated_at": "01/01/2026 10:00", "hash": "a", "fetched_at": 1790000000, "attachments": ["guia.txt"]},
          "2": {"title": "$PUBLIC_TITLE", "area": "Acesso", "visibility": "publica", "tags": ["conta"],
                "updated_at": null, "hash": "b", "fetched_at": 1790000000, "attachments": []}
        }
    """.trimIndent()

    private val first = """
        ---
        id: 1
        titulo: "$PRIVATE_TITLE"
        url: "https://suap.exemplo.br/centralservicos/baseconhecimento/1/"
        area: "Tecnologia"
        visibilidade: "privada"
        atualizado_por: "Pessoa Zero"
        ativo: true
        necessita_correcao: true
        supervisao_pendente: false
        tags: ["impressora","toner"]
        servicos: []
        grupos: []
        anexos: ["guia.txt"]
        baixado_em: "2026-10-20T14:05:00Z"
        ---

        # $PRIVATE_TITLE

        ## Resumo

        Procedimento para a **impressora** do setor, com [o guia](../anexos/1/guia.txt).

        - abrir a tampa
          - retirar o cartucho
        - encaixar o novo

        ![esquema](../anexos/1/imagens/esquema.png)

        ## Anexos

        - [guia.txt](../anexos/1/guia.txt)
    """.trimIndent() + "\n"

    private val second = """
        ---
        id: 2
        titulo: "$PUBLIC_TITLE"
        url: "https://suap.exemplo.br/centralservicos/baseconhecimento/2/"
        area: "Acesso"
        visibilidade: "publica"
        tags: ["conta"]
        anexos: []
        baixado_em: "2026-10-20T14:05:00Z"
        ---

        # $PUBLIC_TITLE

        ## Solução

        Abra um chamado de acesso informando a matrícula.
    """.trimIndent() + "\n"

    /** A pasta de dados do perfil em uso (a mesma que o app abre). */
    fun dataDir(context: Context): File = CoreAccess.dataDir(context, Profiles.active(context))

    fun clean(context: Context) {
        dataDir(context).deleteRecursively()
        context.getSharedPreferences("baseconhecimento", Context.MODE_PRIVATE).edit().clear().commit()
    }

    /** Grava a cópia e refaz o índice; devolve a fachada Rust aberta sobre ela. */
    fun install(context: Context): Base {
        clean(context)
        val root = File(dataDir(context), "data/base/default")
        File(root, "entradas").mkdirs()
        File(root, "anexos/1/imagens").mkdirs()
        File(root, "manifesto.json").writeText(manifest)
        File(root, "entradas/1-trocar-o-toner.md").writeText(first)
        File(root, "entradas/2-solicitar-conta.md").writeText(second)
        File(root, "anexos/1/guia.txt").writeText("Para trocar o cartucho de toner, desligue a impressora.")
        File(root, "anexos/1/imagens/esquema.png").writeBytes(android.util.Base64.decode(PNG, android.util.Base64.DEFAULT))
        val base = Base(dataDir(context).absolutePath, Profiles.active(context).url)
        check(base.rebuildIndex() == 2u)
        return base
    }
}
