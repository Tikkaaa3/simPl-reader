package io.github.tikkaaa3.simpl

import android.system.ErrnoException
import android.system.OsConstants
import io.github.tikkaaa3.simpl.core.CoreException

internal enum class FailureAction { Import, Read, Save }

/** Keep platform/Rust details out of normal error messages, while retaining a recovery action. */
internal fun userError(error: Throwable, action: FailureAction): String {
    android.util.Log.e("simPl", "${action.name} failed", error)
    val chain = generateSequence(error) { it.cause }.take(12).toList()
    val reason = chain.filterIsInstance<CoreException.Failed>().firstOrNull()?.reason
    val details = chain.joinToString(" ") { if (it is CoreException.Failed) it.reason else it.message.orEmpty() }.lowercase()
    return when {
        chain.any { it is ErrnoException && it.errno == OsConstants.ENOSPC } ||
            listOf("no space left", "enospc", "os error 28", "disk full").any(details::contains) ->
            "Storage is full. Free some space, then try again. Your changes could not be saved."
        chain.any { it is SecurityException } || listOf("permission denied", "grant access", "cannot read this document", "cannot read this folder").any(details::contains) ->
            "Access to this document was lost. Return to the library and choose it again to grant access."
        listOf("unsupported", "choose an epub", "unknown format").any(details::contains) ->
            "This format is not supported. Choose an EPUB, PDF, HTML, TXT or Markdown file."
        action == FailureAction.Read && reason == "Enter a page number or a printed page label from this book" -> reason
        action == FailureAction.Read && listOf("external link", "link target", "unknown anchor").any(details::contains) ->
            "This link does not point to a readable location in this book. Return to reading."
        action == FailureAction.Save ->
            "Could not save your changes. Check available storage and try again before closing the book."
        action == FailureAction.Import && error is IllegalArgumentException ->
            (error.message ?: "This document cannot be imported.") + " Choose another file or folder."
        listOf("corrupt", "invalid zip", "zip archive", "invalid epub", "invalid pdf", "malformed", "parse", "invalid digit", "json").any(details::contains) || action == FailureAction.Import ->
            "This document is damaged or cannot be read. Try another copy of the file."
        else -> "Could not open this page. Return to the library and reopen the book."
    }
}
