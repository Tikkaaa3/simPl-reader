package io.github.tikkaaa3.simpl

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import android.os.CancellationSignal
import android.content.SharedPreferences
import androidx.core.content.edit
import androidx.core.net.toUri
import androidx.work.*
import io.github.tikkaaa3.simpl.core.*
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.*

internal data class DictionaryOptions(val source: String = "en", val target: String = "tr", val automatic: Boolean = true)

/** One native cache shared by readers, workers and the package manager. */
internal object OfflineDictionary {
    private val ready = java.util.concurrent.CountDownLatch(1)
    @Volatile private var startupFailure: String? = null
    fun coreReady(failure: String?) { startupFailure = failure; ready.countDown() }
    fun awaitReady() {
        check(ready.await(10, java.util.concurrent.TimeUnit.SECONDS)) { "The reader core has not started. Reopen the app and retry." }
        check(startupFailure == null) { startupFailure.orEmpty() }
    }
    val store by lazy { DictionaryStore() }
    val languages by lazy { dictionaryLanguages() }
    private val mutable = MutableStateFlow(DictionaryOptions())
    val options = mutable.asStateFlow()
    private lateinit var preferences: SharedPreferences
    fun initialize(context: Context) {
        val prefs = context.getSharedPreferences("dictionary", Context.MODE_PRIVATE)
        preferences = prefs
        val source = prefs.getString("source", "en").orEmpty().takeIf { code -> languages.any { it.code == code } } ?: "en"
        val target = prefs.getString("target", "tr").orEmpty().takeIf { it in languages.first { language -> language.code == source }.targets }
            ?: languages.first { it.code == source }.targets.first()
        mutable.value = DictionaryOptions(source, target, prefs.getBoolean("automatic", true))
        // Release durable SAF grants even if a queued import was cancelled before
        // its worker started. Jobs survive process death; this observer restarts.
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            WorkManager.getInstance(context).getWorkInfosByTagFlow(DictionaryJobs.TAG).collect { jobs ->
                val finished = jobs.filter { it.state.isFinished }.map { it.id.toString() }.toSet()
                finished.forEach { File(context.cacheDir, "dictionary-jobs/$it.zip").delete() }
                synchronized(DictionaryJobs.grantsLock) {
                    val grants = context.getSharedPreferences("dictionary-grants", Context.MODE_PRIVATE)
                    val pending = grants.all.filterKeys { it !in finished }.values.toSet()
                    grants.all.filterKeys { it in finished }.forEach { (key, value) ->
                        if (value !in pending) try {
                            context.contentResolver.releasePersistableUriPermission((value as String).toUri(), android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        } catch (_: SecurityException) { /* Already revoked by the provider. */ }
                        grants.edit(commit = true) { remove(key) }
                    }
                }
            }
        }
    }
    fun configure(value: DictionaryOptions) {
        val source = languages.first { it.code == value.source }
        val validated = value.copy(target = value.target.takeIf { it in source.targets } ?: source.targets.first())
        preferences.edit { putString("source", validated.source); putString("target", validated.target); putBoolean("automatic", validated.automatic) }
        mutable.value = validated
    }
}

/** Durable, bounded jobs. Work input never contains document text or a URL. */
internal object DictionaryJobs {
    val grantsLock = Any()
    const val TAG = "dictionary-packages"
    fun download(context: Context, id: UInt) {
        val request = OneTimeWorkRequestBuilder<DictionaryWorker>()
            .setInputData(workDataOf("package" to id.toInt()))
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 10, java.util.concurrent.TimeUnit.SECONDS)
            .addTag(TAG).addTag("dictionary-package-$id").addTag("dictionary-created-${System.currentTimeMillis()}").build()
        WorkManager.getInstance(context).enqueueUniqueWork("dictionary-$id", ExistingWorkPolicy.KEEP, request)
    }
    fun import(context: Context, uri: Uri) = synchronized(grantsLock) {
        require(uri.scheme == "content") { "Choose a ZIP from a storage provider." }
        context.contentResolver.takePersistableUriPermission(uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
        val request = OneTimeWorkRequestBuilder<DictionaryWorker>().setInputData(workDataOf("uri" to uri.toString()))
            .addTag(TAG).addTag("dictionary-import-job").addTag("dictionary-created-${System.currentTimeMillis()}").build()
        context.getSharedPreferences("dictionary-grants", Context.MODE_PRIVATE).edit(commit = true) { putString(request.id.toString(), uri.toString()) }
        WorkManager.getInstance(context).enqueueUniqueWork("dictionary-import", ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }
}

class DictionaryWorker(context: Context, parameters: WorkerParameters) : Worker(context, parameters) {
    private val cancellation by lazy { DictionaryCancellation() }
    @Volatile private var connection: HttpURLConnection? = null
    @Volatile private var stream: java.io.InputStream? = null
    private val providerCancellation = CancellationSignal()
    private fun active() { if (isStopped) { cancellation.cancel(); throw CancellationException("Dictionary job cancelled.") } }
    override fun onStopped() {
        cancellation.cancel(); providerCancellation.cancel(); connection?.disconnect()
        try { stream?.close() } catch (_: IOException) { /* Already closed by the worker. */ }
    }

    override fun doWork(): Result {
        val uri = inputData.getString("uri")?.toUri()
        val stage = File(applicationContext.cacheDir, "dictionary-jobs/$id.zip")
        return try {
            // WorkManager's startup provider precedes Application.onCreate.
            // Never capture a native store before its Android roots are configured.
            OfflineDictionary.awaitReady()
            active()
            check(stage.parentFile!!.mkdirs() || stage.parentFile!!.isDirectory)
            if (uri != null) {
                require(uri.scheme == "content") { "Choose a ZIP from a storage provider." }
                applicationContext.contentResolver.openAssetFileDescriptor(uri, "r", providerCancellation)?.use { descriptor -> descriptor.createInputStream().use { input ->
                    stream = input
                    stage.outputStream().use { output ->
                        val buffer = ByteArray(32 * 1024); var bytes = 0L
                        while (true) {
                            active(); val count = input.read(buffer); if (count < 0) break
                            bytes += count; require(bytes <= 8 * 1024 * 1024) { "Dictionary ZIP exceeds the size limit." }
                            output.write(buffer, 0, count)
                        }
                    }
                } } ?: error("Cannot read this ZIP. Choose it again to grant access.")
                active(); progress(0, 0, "Verifying ZIP")
                val installed = OfflineDictionary.store.importPackage(stage.absolutePath, cancellation)
                Result.success(workDataOf("package" to installed.toInt(), "message" to "Dictionary installed. Ready offline."))
            } else {
                val packageId = inputData.getInt("package", -1)
                val pack = OfflineDictionary.store.inventory().firstOrNull { it.id.toInt() == packageId }
                    ?: error("Unknown dictionary package.")
                download(pack, stage)
                active(); progress(pack.bytes.toLong(), pack.bytes.toLong(), "Verifying ZIP")
                OfflineDictionary.store.installPackage(pack.id, stage.absolutePath, cancellation)
                Result.success(workDataOf("package" to packageId, "message" to "Dictionary installed. Ready offline."))
            }
        } catch (error: CancellationException) {
            throw error
        } catch (error: IOException) {
            if (isStopped) throw CancellationException("Dictionary job cancelled.")
            if (uri == null && runAttemptCount < 2) {
                progress(0, 0, "Connection failed. Retrying…"); Result.retry()
            } else Result.failure(workDataOf("message" to "Could not read dictionary data. Check your connection or choose the ZIP again."))
        } catch (error: Exception) {
            Result.failure(workDataOf("message" to (error.message ?: "Could not install dictionary.")))
        } finally {
            stream = null; connection?.disconnect(); connection = null; stage.delete()
        }
    }

    private fun progress(bytes: Long, total: Long, message: String) {
        setProgressAsync(workDataOf("bytes" to bytes, "total" to total, "message" to message)).get()
    }
    private fun download(pack: DictionaryPackage, stage: File) {
        require(pack.bytes.toLong() in 1..8 * 1024 * 1024) { "Dictionary package exceeds the download limit." }
        val started = SystemClock.elapsedRealtime()
        fun checkTime() { active(); if (SystemClock.elapsedRealtime() - started > 180_000) throw IOException("Download timed out.") }
        var url = URL(pack.url)
        for (redirect in 0..5) {
            checkTime()
            require(url.protocol == "https" && url.host in setOf("github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com") &&
                url.userInfo == null && url.port in listOf(-1, 443)) { "Invalid dictionary download address." }
            val request = (url.openConnection() as HttpURLConnection).apply {
                instanceFollowRedirects = false; connectTimeout = 5000; readTimeout = 5000
                setRequestProperty("User-Agent", "simPl dictionary downloader/1")
                setRequestProperty("Accept-Encoding", "identity")
            }
            connection = request
            try {
                val status = request.responseCode
                if (status in listOf(301, 302, 303, 307, 308)) {
                    require(redirect < 5) { "Too many dictionary redirects." }
                    url = URL(url, request.getHeaderField("Location") ?: error("Missing download address.")); continue
                }
                if (status == 429 || status in 500..599) throw IOException("Release host temporarily unavailable.")
                require(status == 200) { "Dictionary download returned HTTP $status. Try again later." }
                val expected = pack.bytes.toLong()
                require(request.contentLengthLong < 0 || request.contentLengthLong == expected) { "Dictionary download size does not match this version." }
                request.inputStream.use { input -> stage.outputStream().use { output ->
                    val buffer = ByteArray(32 * 1024); var bytes = 0L; var lastPercent = -1L
                    while (true) {
                        checkTime(); val count = input.read(buffer); if (count < 0) break
                        bytes += count; require(bytes <= expected) { "Downloaded dictionary is larger than expected." }
                        output.write(buffer, 0, count)
                        val percent = bytes * 100 / expected
                        if (percent != lastPercent) { progress(bytes, expected, "Downloading ${pack.label}"); lastPercent = percent }
                    }
                    if (bytes != expected) throw IOException("Incomplete dictionary download.")
                } }
                return
            } finally { request.disconnect(); connection = null }
        }
        error("Too many dictionary redirects.")
    }
}
