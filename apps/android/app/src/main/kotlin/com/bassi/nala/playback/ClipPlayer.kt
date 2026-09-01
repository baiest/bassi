package com.bassi.nala.playback

import android.content.Context
import android.media.MediaPlayer
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import kotlin.concurrent.thread

/**
 * Plays WAV clips one at a time, in the order they're enqueued — so
 * narration clips never overlap the final reply. Each clip is written to
 * a temp file because `MediaPlayer` needs a file/URI data source, not raw
 * bytes.
 */
class ClipPlayer(private val context: Context) {
    private val queue = LinkedBlockingQueue<ByteArray>()
    private val worker = thread(name = "nala-playback") {
        try {
            while (true) {
                playBlocking(queue.take())
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
            done.await()
        } catch (_: Exception) {
            // A single bad clip shouldn't kill the playback queue.
        } finally {
            file.delete()
        }
    }
}
