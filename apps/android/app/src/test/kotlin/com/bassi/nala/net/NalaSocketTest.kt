package com.bassi.nala.net

import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class NalaSocketTest {

    private lateinit var server: MockWebServer

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    @Test
    fun `forwards every binary clip received in order`() {
        val received = CopyOnWriteArrayList<String>()
        val allReceived = CountDownLatch(2)

        server.enqueue(
            MockResponse().withWebSocketUpgrade(
                object : WebSocketListener() {
                    override fun onOpen(webSocket: WebSocket, response: Response) {
                        webSocket.send("one".toByteArray().toByteString())
                        webSocket.send("two".toByteArray().toByteString())
                    }
                },
            ),
        )

        val client = NalaSocket(OkHttpClient())
        client.connect(
            host = server.hostName,
            port = server.port,
            onClip = { bytes ->
                received.add(String(bytes))
                allReceived.countDown()
            },
            onError = {},
        )

        assertTrue(allReceived.await(5, TimeUnit.SECONDS))
        assertEquals(listOf("one", "two"), received)
    }

    @Test
    fun `sendUtterance transmits the exact bytes given`() {
        val opened = CountDownLatch(1)
        val messageReceived = CountDownLatch(1)
        var receivedBytes: ByteArray? = null

        server.enqueue(
            MockResponse().withWebSocketUpgrade(
                object : WebSocketListener() {
                    override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                        receivedBytes = bytes.toByteArray()
                        messageReceived.countDown()
                    }
                },
            ),
        )

        val client = NalaSocket(OkHttpClient())
        val payload = byteArrayOf(1, 2, 3, 4)
        client.connect(
            host = server.hostName,
            port = server.port,
            onOpen = { opened.countDown() },
            onClip = {},
            onError = {},
        )

        assertTrue(opened.await(5, TimeUnit.SECONDS))
        client.sendUtterance(payload)

        assertTrue(messageReceived.await(5, TimeUnit.SECONDS))
        assertArrayEquals(payload, receivedBytes)
    }

    @Test
    fun `onError fires when the connection cannot be established`() {
        server.shutdown()
        val errorReceived = CountDownLatch(1)

        val client = NalaSocket(OkHttpClient())
        client.connect(
            host = server.hostName,
            port = server.port,
            onClip = {},
            onError = { errorReceived.countDown() },
        )

        assertTrue(errorReceived.await(5, TimeUnit.SECONDS))
    }
}
