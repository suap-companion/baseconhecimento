package br.edu.ifrn.baseconhecimento

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class MarkdownTest {
    private fun plain(runs: List<Run>) = runs.joinToString("") { it.text }

    @Test
    fun headingsParagraphsAndRules() {
        val blocks = Markdown.parse("## Resumo\n\nPrimeira linha\nsegunda linha\n\n---\n\n### Passo")
        assertEquals(Block.Heading(2, listOf(Run("Resumo"))), blocks[0])
        assertEquals(Block.Paragraph(listOf(Run("Primeira linha segunda linha"))), blocks[1])
        assertEquals(Block.Rule, blocks[2])
        assertEquals(3, (blocks[3] as Block.Heading).level)
    }

    @Test
    fun hardBreakKeepsTheLineBreak() {
        val paragraph = Markdown.parse("um  \ndois").single() as Block.Paragraph
        assertEquals("um\ndois", plain(paragraph.runs))
    }

    @Test
    fun inlineStyles() {
        val runs = (Markdown.parse("a **negrito** e *itálico* e ~~riscado~~ e `código`").single() as Block.Paragraph).runs
        assertEquals("a negrito e itálico e riscado e código", plain(runs))
        assertTrue(runs.first { it.text == "negrito" }.bold)
        assertTrue(runs.first { it.text == "itálico" }.italic)
        assertTrue(runs.first { it.text == "riscado" }.strike)
        assertTrue(runs.first { it.text == "código" }.code)
        assertTrue(runs.first { it.text == "a " }.let { !it.bold && !it.italic })
    }

    @Test
    fun nestedStylesAndLoneAsterisks() {
        val runs = (Markdown.parse("**negrito *e itálico***").single() as Block.Paragraph).runs
        assertTrue(runs.any { it.text == "e itálico" && it.bold && it.italic })
        assertEquals("2 * 3 * 4", plain((Markdown.parse("2 * 3 * 4").single() as Block.Paragraph).runs))
        assertEquals("a*b", plain((Markdown.parse("a\\*b").single() as Block.Paragraph).runs))
    }

    @Test
    fun linksCarryTheirTargetAndStyle() {
        val runs = (Markdown.parse("veja [o **guia**](../anexos/1/guia%20um.pdf) hoje").single() as Block.Paragraph).runs
        val linked = runs.filter { it.link != null }
        assertEquals("o guia", plain(linked))
        assertEquals("../anexos/1/guia%20um.pdf", linked.first().link)
        assertTrue(linked.any { it.bold })
        assertEquals("veja o guia hoje", plain(runs))
    }

    @Test
    fun imagesBecomeTheirOwnBlocks() {
        val blocks = Markdown.parse("antes ![foto](../anexos/1/imagens/foto%20um.png) depois")
        assertEquals(
            listOf(
                Block.Paragraph(listOf(Run("antes "))),
                Block.Picture("foto", "../anexos/1/imagens/foto%20um.png"),
                Block.Paragraph(listOf(Run(" depois"))),
            ),
            blocks,
        )
        assertEquals(listOf(Block.Picture("", "a.png")), Markdown.parse("![](a.png)"))
    }

    @Test
    fun nestedListsKeepDepthAndMarkers() {
        val items = Markdown.parse("- um\n  - dois\n    - três\n- quatro\n\n1. primeiro\n2. segundo").filterIsInstance<Block.Item>()
        assertEquals(listOf(0, 1, 2, 0, 0, 0), items.map { it.depth })
        assertEquals(listOf("•", "◦", "▪", "•", "1.", "2."), items.map { it.marker })
        assertEquals("três", plain(items[2].runs))
    }

    @Test
    fun listItemContinuationsJoinTheItem() {
        val items = Markdown.parse("- um\n  continua aqui\n- dois").filterIsInstance<Block.Item>()
        assertEquals(2, items.size)
        assertEquals("um continua aqui", plain(items[0].runs))
    }

    @Test
    fun quotesAndCode() {
        val blocks = Markdown.parse("> citado\n> mais\n\n```\nlinha 1\n  linha 2\n```\n\nfim")
        assertEquals("citado\nmais", plain((blocks[0] as Block.Quote).runs))
        assertEquals(Block.Code("linha 1\n  linha 2"), blocks[1])
        assertEquals("fim", plain((blocks[2] as Block.Paragraph).runs))
    }

    @Test
    fun tablesHaveHeaderAndRows() {
        val table = Markdown.parse("| A | B |\n| --- | --- |\n| 1 | **2** |\n| 3 | a \\| b |").single() as Block.Table
        assertEquals(listOf("A", "B"), table.header.map(::plain))
        assertEquals(2, table.rows.size)
        assertTrue(table.rows[0][1].single().bold)
        assertEquals("a | b", plain(table.rows[1][1]))
    }

    @Test
    fun aPipeLineWithoutSeparatorIsJustText() {
        val blocks = Markdown.parse("| solto |")
        assertTrue(blocks.single() is Block.Paragraph)
    }

    @Test
    fun emptyAndOddInput() {
        assertTrue(Markdown.parse("").isEmpty())
        assertTrue(Markdown.parse("\n\n  \n").isEmpty())
        assertEquals("[sem destino", plain((Markdown.parse("[sem destino").single() as Block.Paragraph).runs))
        assertEquals("**aberto", plain((Markdown.parse("**aberto").single() as Block.Paragraph).runs))
        assertEquals("`aberto", plain((Markdown.parse("`aberto").single() as Block.Paragraph).runs))
    }

    @Test
    fun paragraphEndsWhenAnotherBlockStarts() {
        val blocks = Markdown.parse("texto\n# Título\n- item")
        assertTrue(blocks[0] is Block.Paragraph && blocks[1] is Block.Heading && blocks[2] is Block.Item)
    }
}
