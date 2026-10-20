package br.edu.ifrn.baseconhecimento

/** Um trecho de texto com o mesmo estilo; `link` é o destino quando o trecho é um link. */
data class Run(
    val text: String,
    val bold: Boolean = false,
    val italic: Boolean = false,
    val strike: Boolean = false,
    val code: Boolean = false,
    val link: String? = null,
)

/** Um bloco do documento, já pronto para desenhar. */
sealed interface Block {
    data class Heading(val level: Int, val runs: List<Run>) : Block
    data class Paragraph(val runs: List<Run>) : Block

    /** Item de lista; `marker` é o "•" ou o "1." e `depth` o nível de aninhamento (0 é o primeiro). */
    data class Item(val depth: Int, val marker: String, val runs: List<Run>) : Block
    data class Quote(val runs: List<Run>) : Block
    data class Code(val text: String) : Block
    data class Table(val header: List<List<Run>>, val rows: List<List<List<Run>>>) : Block
    data class Picture(val alt: String, val src: String) : Block
    data object Rule : Block
}

/** Algo dentro de uma linha: texto estilizado ou uma imagem. */
sealed interface Inline {
    data class Text(val run: Run) : Inline
    data class Image(val alt: String, val src: String) : Inline
}

/**
 * Leitor do Markdown que o `baseconhecimento` grava (o subconjunto que o conversor do HTML do SUAP produz):
 * títulos, parágrafos, negrito, itálico, tachado, código, links, imagens, listas aninhadas, citações, blocos
 * de código, tabelas e linhas horizontais. Não executa nem baixa nada: só entende o texto.
 */
object Markdown {
    private val HEADING = Regex("""^(#{1,6})\s+(.*?)\s*#*\s*$""")
    private val RULE = Regex("""^\s*([-*_])(\s*\1){2,}\s*$""")
    private val ITEM = Regex("""^(\s*)([-*+]|\d+[.)])\s+(.*)$""")
    private val TABLE_SEPARATOR = Regex("""^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$""")
    private val IMAGE = Regex("""!\[([^\]]*)]\(([^)\s]+)(?:\s+"[^"]*")?\)""")
    private val LINK = Regex("""\[((?:[^\[\]]|\[[^\]]*])*)]\(([^)\s]*)(?:\s+"[^"]*")?\)""")
    private val CELL_SPLIT = Regex("""(?<!\\)\|""")
    private const val ESCAPABLE = "\\`*[]()!|~#_>-+."

    fun parse(text: String): List<Block> {
        val lines = text.replace("\r\n", "\n").split('\n')
        val blocks = mutableListOf<Block>()
        val indents = mutableListOf<Int>()
        var i = 0
        while (i < lines.size) {
            val line = lines[i]
            val item = ITEM.matchEntire(line)
            when {
                line.isBlank() -> i++
                line.trimStart().startsWith("```") -> {
                    val code = mutableListOf<String>()
                    i++
                    while (i < lines.size && !lines[i].trimStart().startsWith("```")) code += lines[i++]
                    i++
                    indents.clear()
                    blocks += Block.Code(code.joinToString("\n"))
                }
                HEADING.matches(line) -> {
                    val (marks, content) = HEADING.matchEntire(line)!!.destructured
                    indents.clear()
                    blocks += flat(content).let { Block.Heading(marks.length, it) }
                    i++
                }
                RULE.matches(line) && item == null -> {
                    indents.clear()
                    blocks += Block.Rule
                    i++
                }
                line.trimStart().startsWith("|") && i + 1 < lines.size && TABLE_SEPARATOR.matches(lines[i + 1]) -> {
                    val rows = mutableListOf(cells(line))
                    i += 2
                    while (i < lines.size && lines[i].trimStart().startsWith("|")) rows += cells(lines[i++])
                    indents.clear()
                    blocks += Block.Table(rows.first(), rows.drop(1))
                }
                line.trimStart().startsWith(">") -> {
                    val quoted = mutableListOf<String>()
                    while (i < lines.size && lines[i].trimStart().startsWith(">")) {
                        quoted += lines[i++].trimStart().removePrefix(">").removePrefix(" ")
                    }
                    indents.clear()
                    blocks += Block.Quote(flat(quoted.joinToString("\n")))
                }
                item != null -> {
                    val (indent, marker, first) = item.destructured
                    val level = indent.length
                    while (indents.isNotEmpty() && indents.last() > level) indents.removeAt(indents.lastIndex)
                    if (indents.isEmpty() || indents.last() < level) indents += level
                    val depth = indents.size - 1
                    val parts = mutableListOf(first)
                    i++
                    // Continuação: linhas mais recuadas que o marcador, que não abrem outro item.
                    while (i < lines.size && lines[i].isNotBlank() && ITEM.matchEntire(lines[i]) == null &&
                        lines[i].length - lines[i].trimStart().length > level
                    ) {
                        parts += lines[i++].trim()
                    }
                    val bullet = marker.first().isDigit().not()
                    val shown = if (bullet) listOf("•", "◦", "▪")[minOf(depth, 2)] else marker
                    blocks += withPictures(inline(joinLines(parts))) { Block.Item(depth, shown, it) }
                }
                else -> {
                    val parts = mutableListOf<String>()
                    while (i < lines.size && lines[i].isNotBlank() && (parts.isEmpty() || !startsBlock(lines, i))) {
                        parts += lines[i++]
                    }
                    indents.clear()
                    blocks += withPictures(inline(joinLines(parts))) { Block.Paragraph(it) }
                }
            }
        }
        return blocks
    }

    /** Se a linha abre outro tipo de bloco (então o parágrafo anterior termina nela). */
    private fun startsBlock(lines: List<String>, i: Int): Boolean {
        val line = lines[i]
        val trimmed = line.trimStart()
        return HEADING.matches(line) || trimmed.startsWith("```") || trimmed.startsWith(">") ||
            ITEM.matches(line) || RULE.matches(line) ||
            (trimmed.startsWith("|") && i + 1 < lines.size && TABLE_SEPARATOR.matches(lines[i + 1]))
    }

    /** Une as linhas de um parágrafo: espaço, ou quebra quando a linha termina com dois espaços. */
    private fun joinLines(parts: List<String>): String = buildString {
        parts.forEachIndexed { index, part ->
            if (index > 0) append(if (parts[index - 1].endsWith("  ")) "\n" else " ")
            append(part.trim())
        }
    }

    /** Separa as imagens do texto: cada uma vira o seu bloco, e o texto em volta continua no tipo do bloco. */
    private fun withPictures(inlines: List<Inline>, make: (List<Run>) -> Block): List<Block> {
        val blocks = mutableListOf<Block>()
        val pending = mutableListOf<Run>()
        fun flush() {
            if (pending.any { it.text.isNotBlank() }) blocks += make(pending.toList())
            pending.clear()
        }
        for (piece in inlines) {
            when (piece) {
                is Inline.Text -> pending += piece.run
                is Inline.Image -> {
                    flush()
                    blocks += Block.Picture(piece.alt, piece.src)
                }
            }
        }
        flush()
        return blocks
    }

    private fun cells(line: String): List<List<Run>> =
        line.trim().removePrefix("|").removeSuffix("|").split(CELL_SPLIT).map { flat(it.trim()) }

    /** O texto com estilo, sem imagens (em títulos e células a imagem vira o texto alternativo). */
    private fun flat(text: String): List<Run> = inline(text).map { piece ->
        when (piece) {
            is Inline.Text -> piece.run
            is Inline.Image -> Run(piece.alt)
        }
    }.filter { it.text.isNotEmpty() }

    fun inline(text: String): List<Inline> {
        val out = mutableListOf<Inline>()
        inline(text, Run(""), out)
        return out
    }

    private fun inline(text: String, style: Run, out: MutableList<Inline>) {
        val buffer = StringBuilder()
        fun flush() {
            if (buffer.isNotEmpty()) out += Inline.Text(style.copy(text = buffer.toString()))
            buffer.clear()
        }
        var i = 0
        while (i < text.length) {
            val c = text[i]
            val image = if (c == '!') IMAGE.matchAt(text, i) else null
            val link = if (c == '[') LINK.matchAt(text, i) else null
            when {
                c == '\\' && i + 1 < text.length && text[i + 1] in ESCAPABLE -> {
                    buffer.append(text[i + 1])
                    i += 2
                }
                c == '`' && text.indexOf('`', i + 1) > i + 1 -> {
                    val end = text.indexOf('`', i + 1)
                    flush()
                    out += Inline.Text(style.copy(text = text.substring(i + 1, end), code = true))
                    i = end + 1
                }
                image != null -> {
                    flush()
                    out += Inline.Image(image.groupValues[1], image.groupValues[2])
                    i += image.value.length
                }
                link != null -> {
                    flush()
                    inline(link.groupValues[1], style.copy(link = link.groupValues[2]), out)
                    i += link.value.length
                }
                text.startsWith("**", i) && text.indexOf("**", i + 2) > i + 2 -> {
                    var end = text.indexOf("**", i + 2)
                    // "***" fecha o itálico de dentro e depois o negrito: o negrito usa os dois últimos asteriscos.
                    while (end + 2 < text.length && text[end + 2] == '*') end++
                    flush()
                    inline(text.substring(i + 2, end), style.copy(bold = true), out)
                    i = end + 2
                }
                text.startsWith("~~", i) && text.indexOf("~~", i + 2) > i + 2 -> {
                    val end = text.indexOf("~~", i + 2)
                    flush()
                    inline(text.substring(i + 2, end), style.copy(strike = true), out)
                    i = end + 2
                }
                c == '*' && italicEnd(text, i) > 0 -> {
                    val end = italicEnd(text, i)
                    flush()
                    inline(text.substring(i + 1, end), style.copy(italic = true), out)
                    i = end + 1
                }
                else -> {
                    buffer.append(c)
                    i++
                }
            }
        }
        flush()
    }

    /** Onde fecha o itálico aberto em `start` (um `*` sozinho), ou -1 se não é itálico (ex.: "2 * 3 * 4"). */
    private fun italicEnd(text: String, start: Int): Int {
        if (start + 1 >= text.length || text[start + 1].isWhitespace() || text[start + 1] == '*') return -1
        var j = start + 1
        while (j < text.length) {
            if (text[j] == '*') {
                if (j + 1 < text.length && text[j + 1] == '*') j++ else return if (text[j - 1].isWhitespace()) -1 else j
            }
            j++
        }
        return -1
    }
}
