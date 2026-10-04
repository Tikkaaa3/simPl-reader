@file:OptIn(androidx.compose.ui.text.ExperimentalTextApi::class)

package io.github.tikkaaa3.simpl

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.*
import androidx.compose.ui.text.font.*
import androidx.compose.ui.text.style.*
import androidx.compose.ui.unit.*
import io.github.tikkaaa3.simpl.core.*
import io.github.tikkaaa3.simpl.core.TextStyle as RunStyle
import kotlin.math.roundToInt

private val literata = FontFamily(
    Font(R.font.literata_regular), Font(R.font.literata_medium, FontWeight.Medium),
    Font(R.font.literata_bold, FontWeight.Bold), Font(R.font.literata_italic, style = FontStyle.Italic),
    Font(R.font.literata_bolditalic, FontWeight.Bold, FontStyle.Italic))
private val spectral = FontFamily(
    Font(R.font.spectral_regular), Font(R.font.spectral_medium, FontWeight.Medium),
    Font(R.font.spectral_bold, FontWeight.Bold), Font(R.font.spectral_italic, style = FontStyle.Italic),
    Font(R.font.spectral_bolditalic, FontWeight.Bold, FontStyle.Italic))
private val fira = FontFamily(
    Font(R.font.firasans_regular), Font(R.font.firasans_medium, FontWeight.Medium),
    Font(R.font.firasans_bold, FontWeight.Bold), Font(R.font.firasans_italic, style = FontStyle.Italic),
    Font(R.font.firasans_bolditalic, FontWeight.Bold, FontStyle.Italic))

internal fun rgbaColor(rgba: UInt): Color = Color(((rgba shr 8) or (rgba shl 24)).toInt())

/** Convert source UTF-8 boundaries to Kotlin UTF-16 without splitting a surrogate. */
internal fun byteIndex(text: String, byte: UInt): Int {
    var bytes = 0u
    var index = 0
    while (index < text.length && bytes < byte) {
        val point = text.codePointAt(index)
        val count = when { point <= 0x7f -> 1u; point <= 0x7ff -> 2u; point <= 0xffff -> 3u; else -> 4u }
        if (bytes + count > byte) break
        bytes += count
        index += Character.charCount(point)
    }
    return index
}

internal fun sourceByte(text: String, offset: Int): UInt {
    val iterator = android.icu.text.BreakIterator.getCharacterInstance(java.util.Locale.ROOT)
    iterator.setText(text)
    val at = offset.coerceIn(0, text.length)
    val boundary = if (iterator.isBoundary(at)) at else iterator.preceding(at).coerceAtLeast(0)
    return text.substring(0, boundary).toByteArray(Charsets.UTF_8).size.toUInt()
}

internal fun SourcePoint.compare(other: SourcePoint): Int = compareValuesBy(this, other, { it.section }, { it.row }, { it.byte })
internal fun ReflowSelection.range(section: UInt, row: BookRow): IntRange? {
    val (a, b) = if (from.compare(to) <= 0) from to to else to to from
    val first = SourcePoint(section, row.index, 0u)
    val last = SourcePoint(section, row.index, row.text.orEmpty().toByteArray(Charsets.UTF_8).size.toUInt())
    if (a.compare(last) >= 0 || b.compare(first) <= 0) return null
    val start = if (a.section == section && a.row == row.index) byteIndex(row.text.orEmpty(), a.byte) else 0
    val end = if (b.section == section && b.row == row.index) byteIndex(row.text.orEmpty(), b.byte) else row.text.orEmpty().length
    return start until end
}

internal fun cutLine(cut: ParagraphCut?, row: BookRow, lines: Int, fallback: Int): Int =
    if (cut?.row == row.index) (cut.line.toDouble() * lines / cut.lines.coerceAtLeast(1u).toDouble()).roundToInt().coerceIn(0, lines) else fallback

internal data class MeasuredBookRow(
    val section: UInt, val row: BookRow, val text: TextLayoutResult?,
    val firstLine: Int, val lastLine: Int, val textTop: Float, val textBottom: Float,
    val top: Float, val bottom: Float, val height: Float,
    val startFraction: Float, val endFraction: Float,
)

internal fun measurePage(pages: List<PageContent>, options: LayoutOptions, measurer: TextMeasurer, color: Color, accent: Color): List<MeasuredBookRow> =
    pages.flatMap { page -> measureRows(page.section, page.rows, options, measurer, color, accent, page.layout.startCut, page.layout.endCut) }

internal fun measureRows(section: UInt, rows: List<BookRow>, options: LayoutOptions, measurer: TextMeasurer, color: Color, accent: Color,
    start: ParagraphCut? = null, end: ParagraphCut? = null): List<MeasuredBookRow> = rows.mapNotNull { row ->
    val p = row.presentation
    val logical = row.text.orEmpty()
    val annotated = buildAnnotatedString {
        append(logical)
        row.styles.forEach { run ->
            val bold = run.style == RunStyle.BOLD || run.style == RunStyle.BOLD_ITALIC
            val italic = run.style == RunStyle.ITALIC || run.style == RunStyle.BOLD_ITALIC
            addStyle(SpanStyle(fontWeight = if (bold) FontWeight.Bold else FontWeight.Normal,
                fontStyle = if (italic) FontStyle.Italic else FontStyle.Normal), byteIndex(logical, run.startByte), byteIndex(logical, run.endByte))
        }
        row.semantics.links.forEach { link -> addStyle(SpanStyle(color = accent, textDecoration = TextDecoration.Underline), byteIndex(logical, link.startByte), byteIndex(logical, link.endByte)) }
    }
    val width = (720f - options.margin.toInt() * 2 - p.left - p.right).roundToInt().coerceAtLeast(1)
    val layout = if (row.kind == RowKind.IMAGE) null else measurer.measure(annotated,
        style = androidx.compose.ui.text.TextStyle(color = color, fontSize = p.fontSize.sp, lineHeight = p.lineHeight.sp,
            fontFamily = when (p.family) { "Spectral" -> spectral; "Fira Sans" -> fira; "monospace" -> FontFamily.Monospace; else -> literata },
            fontWeight = if (row.kind == RowKind.HEADING) FontWeight.Medium else FontWeight.Normal,
            textDirection = if (row.rightToLeft) TextDirection.ContentOrRtl else TextDirection.ContentOrLtr,
            textAlign = if (row.rightToLeft) TextAlign.Right else TextAlign.Left,
            platformStyle = PlatformTextStyle(includeFontPadding = false),
            lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None)),
        constraints = Constraints(minWidth = if (row.rightToLeft) width else 0, maxWidth = width), density = Density(1f, 1f), layoutDirection = LayoutDirection.Ltr)
    val lines = layout?.lineCount ?: 1
    val first = cutLine(start, row, lines, 0)
    val last = cutLine(end, row, lines, lines)
    if (last <= first) return@mapNotNull null
    val textTop = layout?.getLineTop(first) ?: 0f
    val textBottom = layout?.getLineBottom(last - 1) ?: p.imageHeight
    val top = if (first == 0) p.top else 0f
    val bottom = if (last == lines) p.bottom + p.gap else 0f
    MeasuredBookRow(section, row, layout, first, last, textTop, textBottom, top, bottom,
        top + textBottom - textTop + bottom, first.toFloat() / lines, last.toFloat() / lines)
}

@Composable
internal fun PaperRow(measured: MeasuredBookRow, scale: Float, color: Color, accent: Color,
    image: suspend (UInt, String) -> ReaderImage?, follow: (UInt, BookLink) -> Unit,
    tap: (Offset) -> Unit, modifier: Modifier = Modifier, selection: ReflowSelection? = null,
    marks: List<ReaderMark> = emptyList(), editMark: (ULong) -> Unit = {}, extend: (SourcePoint) -> Unit = {}) {
    val density = LocalDensity.current.density
    val row = measured.row
    val p = row.presentation
    val layout = measured.text
    val visibleText = layout?.let {
        row.text.orEmpty().substring(it.getLineStart(measured.firstLine), it.getLineEnd(measured.lastLine - 1))
    }.orEmpty()
    val tapHandler by rememberUpdatedState(tap)
    val followHandler by rememberUpdatedState(follow)
    val selectionNow by rememberUpdatedState(selection)
    val marksNow by rememberUpdatedState(marks)
    val editNow by rememberUpdatedState(editMark)
    val extendNow by rememberUpdatedState(extend)
    val links = row.semantics.links.filter { link ->
        val a = byteIndex(row.text.orEmpty(), link.startByte)
        val b = byteIndex(row.text.orEmpty(), link.endByte)
        layout != null && a < layout.getLineEnd(measured.lastLine - 1) && b > layout.getLineStart(measured.firstLine)
    }
    val semantics = Modifier.semantics {
        if (layout != null) text = AnnotatedString(visibleText) else contentDescription = "Book illustration"
        if (row.kind == RowKind.HEADING) heading()
        if (links.isNotEmpty()) customActions = links.map { link ->
            val label = row.text.orEmpty().substring(byteIndex(row.text.orEmpty(), link.startByte), byteIndex(row.text.orEmpty(), link.endByte))
            CustomAccessibilityAction("Follow $label") { followHandler(measured.section, link); true }
        }
    }
    if (layout == null) {
        val bitmap by produceState<ImageBitmap?>(null, measured.section, row.imageAsset) {
            row.imageAsset?.let { asset -> value = runCatching { image(measured.section, asset)?.asBitmap() }.getOrNull() }
        }
        Box(modifier.fillMaxWidth().height((measured.height * scale / density).dp).then(semantics)
            .pointerInput(scale) { detectTapGestures { tapHandler(it) } }, contentAlignment = Alignment.TopCenter) {
            if (bitmap != null) Image(bitmap!!, null, Modifier.width((p.imageWidth * scale / density).dp).height((p.imageHeight * scale / density).dp))
            else Text("Image unavailable", color = color)
        }
    } else Canvas(modifier.fillMaxWidth().height((measured.height * scale / density).dp)
        .testTag("row:${measured.section}:${row.index}").then(semantics)
        .pointerInput(measured, scale) {
            detectTapGestures { point ->
                val source = Offset(point.x / scale - p.left, point.y / scale - measured.top + measured.textTop)
                val offset = layout.getOffsetForPosition(source)
                val mark = marksNow.lastOrNull { offset in byteIndex(row.text.orEmpty(), it.startByte) until byteIndex(row.text.orEmpty(), it.endByte) &&
                    offset < row.text.orEmpty().length && layout.getBoundingBox(offset).contains(source) }
                // TextLayoutResult returns the nearest caret, which may be
                // either side of the hit glyph (including a link's end caret).
                val link = links.firstOrNull { link ->
                    val a = byteIndex(row.text.orEmpty(), link.startByte)
                    val b = byteIndex(row.text.orEmpty(), link.endByte)
                    offset in a..b && listOf(offset, offset - 1).any { index -> index in a until b && layout.getBoundingBox(index).contains(source) }
                }
                if (selectionNow != null) extendNow(SourcePoint(measured.section, row.index, sourceByte(row.text.orEmpty(), offset)))
                else if (mark != null) editNow(mark.id)
                else if (link != null) followHandler(measured.section, link)
                else tapHandler(point)
            }
        }) {
        scale(scale, scale, pivot = Offset.Zero) {
            val width = size.width / scale
            if (row.semantics.kind in listOf(SemanticKind.CODE, SemanticKind.TABLE_ROW, SemanticKind.FORMULA)) {
                drawRect(accent.copy(alpha = 0.045f), size = androidx.compose.ui.geometry.Size(width, measured.height - p.gap))
            }
            if (row.semantics.quoteDepth > 0u) drawLine(accent.copy(alpha = 0.45f), Offset(p.left - 8f, 0f), Offset(p.left - 8f, measured.height - measured.bottom), 2f)
            clipRect(0f, measured.top, width, measured.top + measured.textBottom - measured.textTop) {
                translate(p.left, measured.top - measured.textTop) {
                    coloredRanges(marks.map { (byteIndex(row.text.orEmpty(), it.startByte) until byteIndex(row.text.orEmpty(), it.endByte)) to it.color }).forEach { (range, shade) ->
                        drawPath(layout.getPathForRange(range.first, range.last + 1), shade.tint())
                    }
                    selection?.range(measured.section, row)?.let { range -> drawPath(layout.getPathForRange(range.first, range.last + 1), Color(0x6657a8ef)) }
                    marks.filter { it.note }.forEach { mark ->
                        val start = byteIndex(row.text.orEmpty(), mark.startByte); val end = byteIndex(row.text.orEmpty(), mark.endByte)
                        for (line in layout.getLineForOffset(start)..layout.getLineForOffset((end - 1).coerceAtLeast(start))) {
                            val a = maxOf(start, layout.getLineStart(line)); val b = minOf(end, layout.getLineEnd(line))
                            if (a < b) {
                                val bounds = layout.getPathForRange(a, b).getBounds()
                                drawLine(shadeForNote(mark.color), Offset(bounds.left, layout.getLineBottom(line) - 1), Offset(bounds.right, layout.getLineBottom(line) - 1), 1.5f)
                            }
                        }
                    }
                }
                drawText(layout, topLeft = Offset(p.left, measured.top - measured.textTop))
            }
        }
    }
}

private fun shadeForNote(color: AnnotationColor) = color.tint().copy(alpha = .9f)

private fun ReaderImage.asBitmap(): ImageBitmap {
    val pixels = IntArray(rgba.size / 4) { i ->
        ((rgba[i * 4 + 3].toInt() and 255) shl 24) or ((rgba[i * 4].toInt() and 255) shl 16) or
            ((rgba[i * 4 + 1].toInt() and 255) shl 8) or (rgba[i * 4 + 2].toInt() and 255)
    }
    return android.graphics.Bitmap.createBitmap(pixels, width.toInt(), height.toInt(), android.graphics.Bitmap.Config.ARGB_8888).asImageBitmap()
}
