@file:androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)

package io.github.tikkaaa3.simpl

import android.app.*
import android.content.*
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.os.*
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import androidx.media3.common.*
import androidx.media3.session.*
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import io.github.tikkaaa3.simpl.core.*
import kotlinx.coroutines.*
import java.util.Locale
import java.io.File

/** Synthesizes one bounded chunk, then plays it in this app's audio session. */
class ReadAloudService : MediaSessionService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val handler = Handler(Looper.getMainLooper())
    private var engine: TextToSpeech? = null
    private var ready = false
    private var destroyed = false
    private var plan: SpeechPlan? = null
    private var extracting: Job? = null
    private var chunk: SpeechChunk? = null
    private var offset = 0
    private var spoken = 0
    private var generation = 0L
    private var utterance: String? = null
    private var playback: MediaPlayer? = null
    private var speechFile: File? = null
    private var sampleRate = 0
    private data class Word(val frame: Int, val start: Int, val end: Int)
    private val words = mutableListOf<Word>()
    private var nextWord = 0
    private val timing = object : Runnable {
        override fun run() {
            val output = playback ?: return
            if (!ReadAloud.state.value.playing || ReadAloud.state.value.preparing) return
            val frame = output.currentPosition.toLong() * sampleRate / 1000
            var word: Word? = null
            while (nextWord < words.size && words[nextWord].frame <= frame) word = words[nextWord++]
            word?.let { current ->
                spoken = offset + current.start
                chunk?.let { ReadAloud.mutable.value = ReadAloud.mutable.value.copy(range = speechRange(it, spoken.toUInt(), (offset + current.end).toUInt())) }
            }
            handler.postDelayed(this, 30)
        }
    }
    private var focusResume = false
    private lateinit var audio: AudioManager
    private lateinit var focus: AudioFocusRequest
    private var hasFocus = false
    private lateinit var wake: PowerManager.WakeLock
    private lateinit var player: SpeechPlayer
    private var session: MediaSession? = null
    private var checkpoint: Job? = null
    private val noisy = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) { pauseReading() }
    }

    override fun onCreate() {
        super.onCreate()
        ReadAloud.service = this
        // An interrupted process can leave its one synthesized chunk behind.
        cacheDir.listFiles { file -> file.name.startsWith("speech-") && file.extension == "wav" }?.forEach { it.delete() }
        val prefs = getSharedPreferences("speech", MODE_PRIVATE)
        ReadAloud.mutable.value = ReadAloud.mutable.value.copy(voice = prefs.getString("voice", "").orEmpty(), rate = prefs.getFloat("rate", 1f).coerceIn(.5f, 2f))
        audio = getSystemService(AudioManager::class.java)
        val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
        focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(attributes)
            .setWillPauseWhenDucked(true).setOnAudioFocusChangeListener({ change ->
                when (change) {
                    AudioManager.AUDIOFOCUS_GAIN -> { hasFocus = true; if (focusResume) { focusResume = false; resumeReading() } }
                    AudioManager.AUDIOFOCUS_LOSS_TRANSIENT, AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> {
                        val playing = ReadAloud.state.value.playing; pauseReading(abandon = false); hasFocus = false; focusResume = playing
                    }
                    AudioManager.AUDIOFOCUS_LOSS -> { pauseReading(); focusResume = false }
                }
            }, handler).build()
        wake = getSystemService(PowerManager::class.java).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "simPl:readAloud")
        wake.setReferenceCounted(false)
        if (Build.VERSION.SDK_INT >= 33) registerReceiver(noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), RECEIVER_NOT_EXPORTED)
        else @Suppress("UnspecifiedRegisterReceiverFlag") registerReceiver(noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY))
        player = SpeechPlayer()
        val activity = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).setAction(OPEN_READER), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        session = MediaSession.Builder(this, player).setSessionActivity(activity).build()
        setMediaNotificationProvider(DefaultMediaNotificationProvider.Builder(this).setNotificationId(NOTIFICATION).build())
        // Playback can start directly from the reader before any MediaController
        // binds. Register now so Media3 owns the notification/system session.
        addSession(requireNotNull(session))
        engine = TextToSpeech(this) { status -> handler.post {
            if (destroyed) return@post
            if (status != TextToSpeech.SUCCESS) { fail("No text-to-speech engine is available. Install or enable one in Android settings."); return@post }
            ready = true
            engine?.setAudioAttributes(attributes)
            engine?.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                override fun onStart(id: String?) = Unit
                override fun onBeginSynthesis(id: String?, rate: Int, format: Int, channels: Int) = post(id) { sampleRate = rate }
                override fun onDone(id: String?) = post(id) { playSynthesized() }
                @Deprecated("Legacy engine callback") override fun onError(id: String?) = onError(id, TextToSpeech.ERROR)
                override fun onError(id: String?, errorCode: Int) = post(id) { fail("The speech engine could not read this text. Check the installed voice or choose another voice.") }
                override fun onRangeStart(id: String?, start: Int, end: Int, frame: Int) = post(id) {
                    // File synthesis reports ranges ahead of playback. Schedule
                    // them against the output's sample clock, not callback time.
                    words.add(Word(frame, start, end))
                }
            })
            val voices = engine?.voices.orEmpty().sortedBy { "${it.locale.displayName} ${it.name}" }.map {
                SpeechVoice(it.name, "${it.locale.displayName} · ${it.name}${if (it.isNetworkConnectionRequired) " (network)" else ""}", it.isNetworkConnectionRequired)
            }
            ReadAloud.mutable.value = ReadAloud.mutable.value.copy(voices = voices)
            if (plan != null && ReadAloud.state.value.playing) loadNext()
        } }
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == START) {
            // Foreground status must precede audio focus on Android 15+ and also
            // cover engine initialization/extraction before Media3 sees READY.
            getSystemService(NotificationManager::class.java).createNotificationChannel(NotificationChannel(CHANNEL, "Read aloud", NotificationManager.IMPORTANCE_LOW))
            val notification = Notification.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_stat_read_aloud)
                .setContentTitle(ReadAloud.state.value.title).setContentText("Preparing read aloud…")
                .setContentIntent(session?.sessionActivity).setOngoing(true).build()
            startForeground(NOTIFICATION, notification)
            cancelUtterance(); plan?.close(); plan = ReadAloud.pending; ReadAloud.pending = null
            chunk = null; offset = 0; spoken = 0
            if (plan == null) finish() else if (ready) loadNext()
            player.changed()
        }
        super.onStartCommand(intent, flags, startId)
        return START_NOT_STICKY // Never resume speech unexpectedly after process death.
    }

    private fun post(id: String?, action: () -> Unit) { handler.post { if (id != null && id == utterance && ReadAloud.state.value.playing) action() } }
    private fun cancelUtterance() {
        generation++; stopOutput(); extracting?.cancel(); extracting = null
    }
    private fun stopOutput() {
        utterance = null; engine?.stop(); handler.removeCallbacks(timing)
        playback?.release(); playback = null
        speechFile?.delete(); speechFile = null
        words.clear(); nextWord = 0; sampleRate = 0
    }
    private fun playSynthesized() {
        val file = speechFile ?: return
        val id = utterance ?: return
        try {
            normalizeWords()
            val output = MediaPlayer()
            playback = output
            output.setWakeMode(this, PowerManager.PARTIAL_WAKE_LOCK)
            output.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            output.setDataSource(file.absolutePath)
            output.setOnPreparedListener {
                if (utterance == id && ReadAloud.state.value.playing) {
                    output.start()
                    ReadAloud.mutable.value = ReadAloud.mutable.value.copy(preparing = false)
                    player.changed(); handler.post(timing)
                }
            }
            output.setOnCompletionListener {
                if (utterance == id) {
                    stopOutput(); chunk = null; offset = 0; spoken = 0; loadNext()
                }
            }
            output.setOnErrorListener { _, _, _ ->
                if (utterance == id) fail("Could not play the speech audio. Choose another installed voice.")
                true
            }
            output.prepareAsync()
        } catch (error: Exception) { fail("Could not play the speech audio. Choose another installed voice.") }
    }
    private fun normalizeWords() {
        val text = chunk?.text?.substring(offset).orEmpty()
        fun valid(markers: List<Word>) = markers.isNotEmpty() && markers.all {
            it.frame >= 0 && it.start >= 0 && it.end > it.start && it.end <= text.length && text.substring(it.start, it.end).isNotBlank()
        }
        // AOSP FileSynthesisCallback dispatches (frame, start, end) to a
        // dispatcher expecting (start, end, frame). Validate both layouts so
        // platforms that fix this bug keep using their correct callbacks.
        // https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/speech/tts/FileSynthesisCallback.java
        val normal = words.toList()
        val rotated = normal.map { Word(it.start, it.end, it.frame) }
        val normalized = when {
            valid(normal) -> normal
            valid(rotated) -> rotated
            else -> emptyList() // Unsupported timing: retain paragraph/page follow.
        }
        words.clear(); words.addAll(normalized)
    }
    internal fun prepareReplacement() { cancelUtterance() }
    private fun loadNext() {
        if (!ready || !ReadAloud.state.value.playing) return
        val owned = plan ?: return
        val token = generation
        wake.acquire(120_000)
        ReadAloud.mutable.value = ReadAloud.mutable.value.copy(preparing = true)
        player.changed()
        extracting = scope.launch {
            try {
                // Native next mutates its cursor: let it finish even if paused,
                // then retain that chunk instead of consuming it a second time.
                val next = withContext(Dispatchers.IO + NonCancellable) { owned.next() }
                if (plan !== owned || token != generation) return@launch
                if (next == null) { finish(); return@launch }
                chunk = next; offset = 0; spoken = 0
                if (ReadAloud.state.value.playing) speak()
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) { if (plan === owned) fail("Could not load text for read aloud. Try reopening the book.") }
        }
    }

    private fun speak() {
        val current = chunk ?: return
        // Notification/headset resume can happen after Media3 has demoted a
        // paused service; restore foreground status before requesting focus.
        startForeground(NOTIFICATION, Notification.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_stat_read_aloud)
            .setContentTitle(ReadAloud.state.value.title).setContentText("Reading aloud")
            .setContentIntent(session?.sessionActivity).setOngoing(true).build())
        if (!hasFocus) {
            hasFocus = audio.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
            if (!hasFocus) { pauseReading(); ReadAloud.mutable.value = ReadAloud.mutable.value.copy(error = "Audio is in use. Resume read aloud when it is available."); return }
        }
        val tts = engine ?: return
        val chosen = ReadAloud.state.value.voice
        val voice = if (chosen.isNotBlank()) tts.voices?.find { it.name == chosen }
            else tts.voices?.filter { !it.isNetworkConnectionRequired && it.locale.language == current.language }?.maxByOrNull { it.quality }
        if (voice != null) tts.voice = voice
        else {
            val language = current.language?.let(Locale::forLanguageTag) ?: Locale.getDefault()
            val result = tts.setLanguage(language)
            if (result == TextToSpeech.LANG_MISSING_DATA || result == TextToSpeech.LANG_NOT_SUPPORTED) {
                fail("No installed voice supports this text. Choose a voice or install voice data in Android settings."); return
            }
        }
        tts.setSpeechRate(ReadAloud.state.value.rate)
        wake.acquire(120_000)
        val id = "${generation}:${SystemClock.elapsedRealtimeNanos()}"; utterance = id
        val range = speechRange(current, offset.toUInt(), (offset + 1).toUInt())
        ReadAloud.mutable.value = ReadAloud.mutable.value.copy(chunk = current, preparing = true, range = range)
        saveCheckpoint(range)
        // TTS.speak renders in the engine's UID, which prevents Android from
        // routing headset keys to our MediaSession. Own the actual audio output.
        try {
            speechFile = File.createTempFile("speech-", ".wav", cacheDir)
            if (tts.synthesizeToFile(current.text.substring(offset), Bundle(), requireNotNull(speechFile), id) == TextToSpeech.ERROR) {
                fail("The speech engine could not start. Choose another installed voice.")
            }
        } catch (error: Exception) { fail("Could not prepare speech audio. Check available storage and try again.") }
        player.changed()
    }

    internal fun pauseReading(abandon: Boolean = true) {
        if (!ReadAloud.state.value.active) return
        // Android TTS has no pause: stop, retaining the last reported word.
        stopOutput(); offset = spoken.coerceIn(0, chunk?.text?.length ?: 0)
        ReadAloud.mutable.value = ReadAloud.mutable.value.copy(playing = false, preparing = false)
        ReadAloud.state.value.range?.let(::saveCheckpoint)
        if (wake.isHeld) wake.release()
        if (abandon) { focusResume = false; audio.abandonAudioFocusRequest(focus); hasFocus = false }
        player.changed()
    }
    internal fun resumeReading() {
        if (!ReadAloud.state.value.active || ReadAloud.state.value.playing) return
        ReadAloud.mutable.value = ReadAloud.mutable.value.copy(playing = true, error = null)
        if (chunk != null) speak() else if (extracting?.isActive != true) loadNext()
        player.changed()
    }
    internal fun options(voice: String, rate: Float) {
        val speed = rate.coerceIn(.5f, 2f)
        getSharedPreferences("speech", MODE_PRIVATE).edit().putString("voice", voice).putFloat("rate", speed).apply()
        ReadAloud.mutable.value = ReadAloud.mutable.value.copy(voice = voice, rate = speed)
        if (chunk != null && ReadAloud.state.value.playing) { stopOutput(); offset = spoken; speak() }
    }
    internal fun finish() {
        cancelUtterance(); plan?.close(); plan = null; chunk = null
        if (wake.isHeld) wake.release()
        audio.abandonAudioFocusRequest(focus); hasFocus = false; focusResume = false
        ReadAloud.mutable.value = ReadAloud.mutable.value.copy(active = false, playing = false, preparing = false, chunk = null, range = null)
        player.changed(); stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
    }
    private fun saveCheckpoint(range: SpeechRange) {
        val owned = plan ?: return
        checkpoint?.cancel()
        checkpoint = scope.launch {
            runCatching { withContext(Dispatchers.IO) { owned.checkpoint(range) } }
                .onFailure { android.util.Log.w("simPl", "Could not save speech position", it) }
        }
    }
    private fun fail(message: String) { finish(); ReadAloud.mutable.value = ReadAloud.mutable.value.copy(error = message) }
    override fun onTaskRemoved(rootIntent: Intent?) { if (!ReadAloud.state.value.playing) finish() }
    override fun onDestroy() {
        destroyed = true
        finish(); session?.release(); session = null; player.release()
        unregisterReceiver(noisy); engine?.shutdown(); engine = null; ready = false; scope.cancel()
        if (ReadAloud.service === this) ReadAloud.service = null
        super.onDestroy()
    }

    private inner class SpeechPlayer : SimpleBasePlayer(Looper.getMainLooper()) {
        fun changed() = invalidateState()
        override fun getState(): State {
            val current = ReadAloud.state.value
            val commands = Player.Commands.Builder().addAll(Player.COMMAND_PLAY_PAUSE, Player.COMMAND_STOP,
                Player.COMMAND_GET_CURRENT_MEDIA_ITEM, Player.COMMAND_GET_TIMELINE, Player.COMMAND_GET_METADATA).build()
            val builder = State.Builder().setAvailableCommands(commands).setPlayWhenReady(current.active && current.playing, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
                .setPlaybackState(if (!current.active) Player.STATE_IDLE else if (current.preparing) Player.STATE_BUFFERING else Player.STATE_READY)
            if (current.active) builder.setPlaylist(listOf(MediaItemData.Builder(current.fingerprint)
                .setMediaItem(MediaItem.Builder().setMediaId(current.fingerprint).setMediaMetadata(MediaMetadata.Builder().setTitle(current.title).setArtist("Read aloud").build()).build()).build()))
            return builder.build()
        }
        override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
            if (playWhenReady) resumeReading() else pauseReading()
            return Futures.immediateVoidFuture()
        }
        override fun handleStop(): ListenableFuture<*> { finish(); return Futures.immediateVoidFuture() }
        override fun handleRelease(): ListenableFuture<*> = Futures.immediateVoidFuture()
    }
    companion object {
        internal const val START = "io.github.tikkaaa3.simpl.START_READ_ALOUD"
        internal const val OPEN_READER = "io.github.tikkaaa3.simpl.OPEN_SPOKEN_READER"
        private const val CHANNEL = "read_aloud"
        private const val NOTIFICATION = 1001
    }
}
