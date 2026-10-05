package io.github.tikkaaa3.simpl

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*

internal fun readerPageMatcher(label: String, substring: Boolean = false) =
    hasTestTag("pageLabel") and SemanticsMatcher("reader page $label") {
        val actual = it.config.getOrElse(SemanticsProperties.StateDescription) { "" }
        if (substring) actual.contains(label) else actual == label
    }

internal fun SemanticsNodeInteractionsProvider.openReaderTools() {
    if (onAllNodesWithTag("readerTools").fetchSemanticsNodes().isEmpty())
        onNodeWithContentDescription("Reader tools").performClick()
}

internal fun SemanticsNodeInteractionsProvider.readerTool(label: String) {
    openReaderTools()
    onNode(hasText(label) and hasClickAction()).performClick()
}

internal fun SemanticsNodeInteractionsProvider.startReaderSpeech() {
    openReaderTools()
    onNodeWithTag("speechStart").performClick()
}

internal fun SemanticsNodeInteractionsProvider.readerJump(value: String) {
    onNodeWithTag("jumpPage").performClick().performTextReplacement(value)
    onNodeWithTag("jumpPage").performImeAction()
}

internal fun cancelReaderJump() = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
    .sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_ESCAPE)
