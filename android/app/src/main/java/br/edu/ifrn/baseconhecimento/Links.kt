package br.edu.ifrn.baseconhecimento

import java.io.File
import java.net.URI

/** Para onde um link do Markdown leva. */
sealed interface LinkTarget {
    /** Um endereço da web (ou e-mail): abre no navegador. */
    data class Web(val url: String) : LinkTarget

    /** Um arquivo da cópia local (anexo ou imagem), já conferido. */
    data class Local(val file: File) : LinkTarget

    /** Nada que o app abra: caminho que sai da cópia local, arquivo ausente ou esquema desconhecido. */
    data object None : LinkTarget
}

object Links {
    /** Desfaz o `%XX` (UTF-8) de um caminho; o `+` continua sendo `+`. */
    fun decode(path: String): String {
        val bytes = java.io.ByteArrayOutputStream()
        var i = 0
        while (i < path.length) {
            val pair = if (path[i] == '%' && i + 2 < path.length) path.substring(i + 1, i + 3) else null
            val hex = pair?.takeIf { p -> p.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' } }?.toInt(16)
            if (hex != null) {
                bytes.write(hex)
                i += 3
            } else {
                bytes.write(path[i].toString().toByteArray(Charsets.UTF_8))
                i++
            }
        }
        return bytes.toString(Charsets.UTF_8.name())
    }

    /**
     * Resolve um link ou uma imagem do Markdown. Endereços `http(s)` e `mailto` abrem no navegador; caminhos
     * relativos valem em relação à pasta das entradas e nunca podem sair da cópia local (`root`).
     */
    fun resolve(target: String, entriesDir: File, root: File): LinkTarget {
        val lower = target.lowercase()
        if (lower.startsWith("http://") || lower.startsWith("https://") || lower.startsWith("mailto:")) {
            return LinkTarget.Web(target)
        }
        if (target.isBlank() || ':' in target.substringBefore('/') || target.startsWith("/") || target.startsWith("#")) {
            return LinkTarget.None
        }
        val file = runCatching { File(entriesDir, decode(target.substringBefore('#').substringBefore('?'))).canonicalFile }
            .getOrNull() ?: return LinkTarget.None
        val inside = file.path.startsWith(root.canonicalFile.path + File.separator)
        return if (inside && file.isFile) LinkTarget.Local(file) else LinkTarget.None
    }
}

/** Já saiu da tela de login, ainda no site do SUAP (o login do gov.br fica em outro endereço). */
fun isAfterLogin(base: String, url: String): Boolean {
    val (site, page) = runCatching { URI(base) to URI(url) }.getOrNull() ?: return false
    return page.authority == site.authority && !page.path.startsWith("/accounts/")
}
