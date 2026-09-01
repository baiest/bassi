package com.bassi.nala.net

import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString

/**
 * One WebSocket connection to `voice --serve`: sends a single WAV
 * utterance and forwards every binary clip that comes back — narration
 * clips, then the final reply — to [onClip], in the order it arrives.
 * `voice`'s protocol is audio-only (see BAS-30), so any non-binary frame
 * is simply not something this client sends or expects to receive.
 *
 * Defaults to a 20s ping interval so a silently dropped connection (e.g.
 * the phone's WiFi going out of range without a clean close) surfaces as
 * [WebSocketListener.onFailure] promptly instead of hanging forever.
 */
class NalaSocket(
    private val client: OkHttpClient = OkHttpClient.Builder()
        .pingInterval(20, TimeUnit.SECONDS)
        .build(),
) {
    private var socket: WebSocket? = null

    // Bumped on every `connect()`. A callback from a socket superseded by a
    // newer one (e.g. a late `onFailure` from the connection `connect()`
    // just replaced) is dropped instead of clobbering state a fresher
    // callback already set.
    private val generation = AtomicInteger(0)

    fun connect(
        host: String,
        port: Int,
        onOpen: () -> Unit = {},
        onClip: (ByteArray) -> Unit,
        onError: (Throwable) -> Unit,
        onClosed: () -> Unit = {},
    ) {
        val myGeneration = generation.incrementAndGet()
        fun current() = generation.get() == myGeneration

        val request = Request.Builder().url("ws://$host:$port").build()
        socket = client.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    if (current()) onOpen()
                }

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                    if (current()) onClip(bytes.toByteArray())
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (current()) onError(t)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    if (current()) onClosed()
                }
            },
        )
    }

    /** Sends [wav] as one binary frame. Returns `false` if not connected. */
    fun sendUtterance(wav: ByteArray): Boolean = socket?.send(wav.toByteString(0, wav.size)) ?: false

    fun close() {
        generation.incrementAndGet()
        socket?.close(1000, null)
        socket = null
    }
}
