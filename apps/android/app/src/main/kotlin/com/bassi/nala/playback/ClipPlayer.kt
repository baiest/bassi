package com.bassi.nala.playback

import android.content.Context
import android.media.MediaPlayer
import com.bassi.nala.audio.Amplitude
import com.bassi.nala.audio.WavReader
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import kotlin.concurrent.thread

/** ~50ms at typical TTS sample rates (16-24 kHz) — matches nala-overlay's `playback.rs`. */
private const val WINDOW_SAMPLES = 1024

/**
 * Plays WAV clips one at a time, in the order they're enqueued — so
 * narration clips never overlap the final reply. Each clip is written to
 * a temp file because `MediaPlayer` needs a file/URI data source, not raw
 * bytes.
 */
class ClipPlayer(private val context: Context) {
    private val queue = LinkedBlockingQueue<ByteArray>()

    /**
     * Fired (off the main thread) once playback catches up to an empty
     * queue — i.e. every clip enqueued so far has finished. A reply can
     * arrive as several clips (narration, then the final answer), so this
     * only fires after the last one, not after each individual clip.
     */
    var onQueueDrained: (() -> Unit)? = null

    /**
     * Fired continuously (off the main thread) while a clip plays, so the
     * core can pulse to what's actually being said — 0 the rest of the
     * time. Precomputed from the clip's own samples and walked in step with
     * playback, same approach as nala-overlay's `playback.rs`, rather than
     * read live from the audio output (Android has no simple tap into what
     * `MediaPlayer` is rendering).
     */
    var onAmplitude: (Float) -> Unit = {}

    private val worker = thread(name = "nala-playback") {
        try {
            while (true) {
                playBlocking(queue.take())
                if (queue.isEmpty()) {
                    onQueueDrained?.invoke()
                }
            }
        } catch (_: InterruptedException) {
            // release() was called — exit quietly.
        }
    }

    fun enqueue(wav: ByteArray) {
        queue.put(wav)
    }

    fun release() {
        worker.interrupt()
    }

    private fun playBlocking(wav: ByteArray) {
        val file = File.createTempFile("nala_clip", ".wav", context.cacheDir)
        val done = CountDownLatch(1)
        try {
            FileOutputStream(file).use { it.write(wav) }

            val player = MediaPlayer()
            player.setDataSource(file.absolutePath)
            player.setOnCompletionListener {
                it.release()
                done.countDown()
            }
            player.setOnErrorListener { mp, _, _ ->
                mp.release()
                done.countDown()
                true
            }
            player.prepare()
            player.start()
            walkAmplitude(wav)
            // Catches any remainder past the last full window (rounding, or
            // playback taking slightly longer than the precomputed curve
            // estimated).
            done.await()
        } catch (_: Exception) {
            // A single bad clip shouldn't kill the playback queue.
        } finally {
            onAmplitude(0f)
            file.delete()
        }
    }

    /**
     * Walks the clip's precomputed loudness curve in real time, sleeping
     * between windows so the pace matches actual playback. A clip that
     * fails to decode (unexpected format) just plays silently as far as the
     * core's animation goes — it isn't skipped, unlike a fully corrupt WAV.
     */
    private fun walkAmplitude(wav: ByteArray) {
        val decoded = try {
            WavReader.decode(wav)
        } catch (_: WavReader.DecodeError) {
            return
        }

        val windows = Amplitude.windows(decoded.samples, WINDOW_SAMPLES * decoded.channels)
        val windowDurationMs = (WINDOW_SAMPLES * 1000L) / decoded.sampleRate.coerceAtLeast(1)
        for (level in windows) {
            onAmplitude(level)
            Thread.sleep(windowDurationMs)
        }
    }
}
