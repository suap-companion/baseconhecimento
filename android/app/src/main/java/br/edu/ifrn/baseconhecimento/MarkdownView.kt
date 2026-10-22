package br.edu.ifrn.baseconhecimento

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Maior lado, em pixels, com que uma imagem do texto é carregada (poupa memória). */
private const val MAX_IMAGE_SIDE = 1600

/** Os trechos como texto estilizado; links viram toques que chamam `onLink` com o destino. */
fun annotated(runs: List<Run>, linkColor: Color, codeBackground: Color, onLink: (String) -> Unit): AnnotatedString =
    buildAnnotatedString {
        for (run in runs) {
            val decoration = buildList {
                if (run.strike) add(TextDecoration.LineThrough)
            }.let { if (it.isEmpty()) null else TextDecoration.combine(it) }
            val style = SpanStyle(
                fontWeight = if (run.bold) FontWeight.Bold else null,
                fontStyle = if (run.italic) FontStyle.Italic else null,
                fontFamily = if (run.code) FontFamily.Monospace else null,
                background = if (run.code) codeBackground else Color.Unspecified,
                textDecoration = decoration,
            )
            val link = run.link
            if (link == null) {
                withStyle(style) { append(run.text) }
            } else {
                val look = TextLinkStyles(SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline))
                withLink(LinkAnnotation.Clickable("link", look) { onLink(link) }) { withStyle(style) { append(run.text) } }
            }
        }
    }

/**
 * O Markdown da entrada, desenhado. As imagens e os anexos vêm só da cópia local (nada é baixado aqui); o que
 * `onOpen` recebe é um arquivo conferido da cópia local ou um endereço da web, para o app abrir fora.
 */
@Composable
fun MarkdownView(markdown: String, entriesDir: File, root: File, onOpen: (LinkTarget) -> Unit, modifier: Modifier = Modifier) {
    val blocks = remember(markdown) { Markdown.parse(markdown) }
    val link = MaterialTheme.colorScheme.primary
    val code = MaterialTheme.colorScheme.surfaceContainerHighest
    val open: (String) -> Unit = { target -> onOpen(Links.resolve(target, entriesDir, root)) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        for (block in blocks) {
            when (block) {
                is Block.Heading -> Text(
                    annotated(block.runs, link, code, open),
                    style = when (block.level) {
                        1, 2 -> MaterialTheme.typography.titleLarge
                        3 -> MaterialTheme.typography.titleMedium
                        else -> MaterialTheme.typography.titleSmall
                    }.copy(fontWeight = FontWeight.Bold),
                    modifier = Modifier.padding(top = 6.dp),
                )
                is Block.Paragraph -> Text(annotated(block.runs, link, code, open), style = MaterialTheme.typography.bodyLarge)
                is Block.Item -> Row(Modifier.padding(start = (block.depth * 18).dp)) {
                    Text(block.marker, Modifier.width(26.dp), style = MaterialTheme.typography.bodyLarge)
                    Text(annotated(block.runs, link, code, open), style = MaterialTheme.typography.bodyLarge)
                }
                is Block.Quote -> Row(Modifier.height(IntrinsicSize.Min)) {
                    Box(Modifier.width(4.dp).fillMaxHeight().background(MaterialTheme.colorScheme.outlineVariant))
                    Text(
                        annotated(block.runs, link, code, open),
                        style = MaterialTheme.typography.bodyLarge.copy(fontStyle = FontStyle.Italic),
                        modifier = Modifier.padding(start = 12.dp),
                    )
                }
                is Block.Code -> Text(
                    block.text,
                    style = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                    modifier = Modifier.fillMaxWidth().background(code).horizontalScroll(rememberScrollState()).padding(10.dp),
                )
                is Block.Table -> TableView(block, link, code, open)
                is Block.Picture -> PictureView(block, entriesDir, root, onOpen)
                Block.Rule -> HorizontalDivider(Modifier.padding(vertical = 4.dp))
            }
        }
    }
}

@Composable
private fun TableView(table: Block.Table, link: Color, code: Color, open: (String) -> Unit) {
    val border = MaterialTheme.colorScheme.outlineVariant
    Column(Modifier.fillMaxWidth().border(1.dp, border)) {
        (listOf(table.header) + table.rows).forEachIndexed { index, row ->
            Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
                for (cell in row) {
                    Text(
                        annotated(cell, link, code, open),
                        style = MaterialTheme.typography.bodyMedium.copy(fontWeight = if (index == 0) FontWeight.Bold else FontWeight.Normal),
                        modifier = Modifier.weight(1f).border(0.5.dp, border).padding(6.dp),
                    )
                }
            }
        }
    }
}

/** Uma imagem da cópia local; a que não pôde ser baixada na atualização aparece só como texto. */
@Composable
private fun PictureView(picture: Block.Picture, entriesDir: File, root: File, onOpen: (LinkTarget) -> Unit) {
    val target = remember(picture.src) { Links.resolve(picture.src, entriesDir, root) }
    val bitmap by produceState<ImageBitmap?>(null, target) {
        value = (target as? LinkTarget.Local)?.let { local -> withContext(Dispatchers.IO) { decode(local.file) } }
    }
    val shown = bitmap
    if (shown != null) {
        Image(
            bitmap = shown,
            contentDescription = picture.alt.ifBlank { "Imagem da entrada" },
            contentScale = ContentScale.FillWidth,
            modifier = Modifier.fillMaxWidth().clickable { onOpen(target) },
        )
    } else {
        Text(
            "Imagem não disponível offline" + if (picture.alt.isNotBlank()) ": ${picture.alt}" else "",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** Lê a imagem reduzida (sem estourar a memória com fotos grandes); `null` se o arquivo não é uma imagem. */
private fun decode(file: File): ImageBitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / sample > MAX_IMAGE_SIDE) sample *= 2
    val options = BitmapFactory.Options().apply { inSampleSize = sample }
    return BitmapFactory.decodeFile(file.path, options)?.asImageBitmap()
}
