package br.edu.ifrn.baseconhecimento

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class LinksTest {
    @get:Rule
    val folder = TemporaryFolder()

    @Test
    fun decodesPercentSequencesAsUtf8() {
        assertEquals("guia um.pdf", Links.decode("guia%20um.pdf"))
        assertEquals("açaí.txt", Links.decode("a%C3%A7a%C3%AD.txt"))
        assertEquals("a+b%zz%", Links.decode("a+b%zz%"))
        assertEquals("100%", Links.decode("100%"))
    }

    @Test
    fun webLinksOpenOutside() {
        val root = folder.newFolder("base")
        for (url in listOf("https://suap.exemplo.br/x", "HTTP://a.b", "mailto:a@b.c")) {
            assertEquals(LinkTarget.Web(url), Links.resolve(url, root, root))
        }
    }

    @Test
    fun relativeLinksStayInsideTheLocalCopy() {
        val root = folder.newFolder("base")
        val entries = File(root, "entradas").apply { mkdirs() }
        val attachment = File(root, "anexos/1/guia um.txt").apply {
            parentFile!!.mkdirs()
            writeText("x")
        }
        File(folder.root, "fora.txt").writeText("segredo")
        assertEquals(LinkTarget.Local(attachment.canonicalFile), Links.resolve("../anexos/1/guia%20um.txt", entries, root))
        assertEquals(LinkTarget.Local(attachment.canonicalFile), Links.resolve("../anexos/1/guia%20um.txt?x=1#y", entries, root))
        // Sai da cópia local, não existe, é pasta, ou não é um caminho relativo: nada abre.
        assertEquals(LinkTarget.None, Links.resolve("../../fora.txt", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("../anexos/1/outro.txt", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("../anexos", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("/etc/passwd", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("file:///etc/passwd", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("javascript:alert(1)", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("#topo", entries, root))
        assertEquals(LinkTarget.None, Links.resolve("", entries, root))
    }

    @Test
    fun aSiblingFolderWithTheSamePrefixIsOutside() {
        val root = folder.newFolder("base")
        val entries = File(root, "entradas").apply { mkdirs() }
        File(folder.root, "base-outro").mkdirs()
        File(folder.root, "base-outro/x.txt").writeText("x")
        assertEquals(LinkTarget.None, Links.resolve("../../base-outro/x.txt", entries, root))
    }
}
