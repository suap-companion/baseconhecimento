package br.edu.ifrn.baseconhecimento

import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** Textos prontos para a tela. */
object Format {
    private val WHEN = DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm", Locale.forLanguageTag("pt-BR"))

    /** "2,5 MB", "340 KB", "12 B". */
    fun size(bytes: Long): String = when {
        bytes >= 1_048_576 -> String.format(Locale.forLanguageTag("pt-BR"), "%.1f MB", bytes / 1_048_576.0)
        bytes >= 1_024 -> String.format(Locale.forLanguageTag("pt-BR"), "%.0f KB", bytes / 1_024.0)
        else -> "$bytes B"
    }

    /** Segundos Unix como "20/10/2026 14:05" no fuso do aparelho; `null` vira "nunca". */
    fun whenText(seconds: Long?, zone: ZoneId = ZoneId.systemDefault()): String =
        seconds?.let { WHEN.format(Instant.ofEpochSecond(it).atZone(zone)) } ?: "nunca"

    /** Quantas entradas, no singular ou no plural. */
    fun entries(count: Int): String = if (count == 1) "1 entrada" else "$count entradas"
}
