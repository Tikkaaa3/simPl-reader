package io.github.tikkaaa3.simpl

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

internal data class SpeechVoice(val id: String, val label: String, val network: Boolean)
internal data class ReadAloudState(
    val fingerprint: String = "", val title: String = "", val active: Boolean = false,
    val playing: Boolean = false, val preparing: Boolean = false, val passage: Boolean = false,
    val chunk: SpeechChunk? = null, val range: SpeechRange? = null,
    val voices: List<SpeechVoice> = emptyList(), val voice: String = "", val rate: Float = 1f,
    val error: String? = null,
)

/** The service owns native plans; reader routes observe source coordinates only. */
internal object ReadAloud {
    internal val mutable = MutableStateFlow(ReadAloudState())
    val state = mutable.asStateFlow()
    internal var service: ReadAloudService? = null
    internal var pending: SpeechPlan? = null
    fun connect(context: Context) {
        runCatching { context.startService(Intent(context, ReadAloudService::class.java)) }
            .onFailure { mutable.value = mutable.value.copy(error = "Could not start read aloud. Please try again.") }
    }
    fun start(context: Context, fingerprint: String, title: String, plan: SpeechPlan, passage: Boolean = false) {
        service?.prepareReplacement()
        pending?.close(); pending = plan
        mutable.value = mutable.value.copy(fingerprint = fingerprint, title = title, active = true,
            playing = true, preparing = true, passage = passage, chunk = null, range = null, error = null)
        try { ContextCompat.startForegroundService(context, Intent(context, ReadAloudService::class.java).setAction(ReadAloudService.START)) }
        catch (error: Exception) {
            pending?.close(); pending = null
            mutable.value = mutable.value.copy(active = false, playing = false, preparing = false,
                error = "Could not start read aloud. Please try again.")
        }
    }
    fun pause() = service?.pauseReading()
    fun resume() = service?.resumeReading()
    fun stop() {
        pending?.close(); pending = null
        service?.finish()
        mutable.value = mutable.value.copy(active = false, playing = false, preparing = false, chunk = null, range = null)
    }
    fun options(voice: String = mutable.value.voice, rate: Float = mutable.value.rate) = service?.options(voice, rate)
    fun dismissError() { mutable.value = mutable.value.copy(error = null) }
}
