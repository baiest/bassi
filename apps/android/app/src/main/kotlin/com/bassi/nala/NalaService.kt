package com.bassi.nala

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.wifi.WifiManager
import android.os.Binder
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import com.bassi.nala.net.NalaSocket
import com.bassi.nala.playback.ClipPlayer
import com.bassi.nala.settings.Prefs

private const val NOTIFICATION_CHANNEL_ID = "nala_connection"
private const val NOTIFICATION_ID = 1

private const val RECONNECT_INITIAL_DELAY_MS = 1_000L
private const val RECONNECT_MAX_DELAY_MS = 30_000L

/**
 * Owns the WebSocket connection to `voice --serve` and clip playback,
 * independent of `MainActivity`'s lifecycle — a reply that arrives after
 * the app is closed still gets played, and reopening the app doesn't need
 * to reconnect. Recording itself still requires the app open (`AudioRecord`
 * lives in `MainActivity`); this service only owns the connection and
 * playback, which is why a `mediaPlayback` foreground type fits it, not
 * `microphone`.
 */
class NalaService : Service() {

    enum class Status { CONNECTING, CONNECTED, DISCONNECTED }

    private val binder = LocalBinder()
    lateinit var socket: NalaSocket
        private set
    private lateinit var clipPlayer: ClipPlayer
    private val reconnectHandler = Handler(Looper.getMainLooper())
    private var reconnectDelayMs = RECONNECT_INITIAL_DELAY_MS
    private val reconnectRunnable = Runnable { connect(resetBackoff = false) }

    // With the screen off, WiFi power-save and CPU doze are what actually
    // kill the socket (the 20s OkHttp ping starts failing) — these two
    // locks are what keeps the connection (and the ability to receive a
    // reply) alive in the background. Held for the service's whole life,
    // not just while CONNECTED, so a drop can still be detected and retried.
    private var wifiLock: WifiManager.WifiLock? = null
    private var wakeLock: PowerManager.WakeLock? = null

    var onStatusChanged: ((Status) -> Unit)? = null
    var onClipReceived: (() -> Unit)? = null
    var onPlaybackFinished: (() -> Unit)? = null

    var status: Status = Status.DISCONNECTED
        private set(value) {
            field = value
            onStatusChanged?.invoke(value)
            updateNotification()
        }

    inner class LocalBinder : Binder() {
        fun service(): NalaService = this@NalaService
    }

    override fun onCreate() {
        super.onCreate()
        socket = NalaSocket()
        clipPlayer = ClipPlayer(applicationContext).apply {
            onQueueDrained = { onPlaybackFinished?.invoke() }
        }
        acquireLocks()
        createNotificationChannel()
        startForegroundWithNotification()
        connect()
    }

    private fun acquireLocks() {
        val wifiManager = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        val wifiLockMode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            WifiManager.WIFI_MODE_FULL_LOW_LATENCY
        } else {
            @Suppress("DEPRECATION")
            WifiManager.WIFI_MODE_FULL_HIGH_PERF
        }
        wifiLock = wifiManager.createWifiLock(wifiLockMode, "nala:wifi").apply {
            setReferenceCounted(false)
            acquire()
        }

        val powerManager = applicationContext.getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "nala:socket").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    private fun releaseLocks() {
        wifiLock?.takeIf { it.isHeld }?.release()
        wifiLock = null
        wakeLock?.takeIf { it.isHeld }?.release()
        wakeLock = null
    }

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int = START_STICKY

    /**
     * Drops any existing connection and opens a fresh one at the saved
     * host/port. [resetBackoff] is true for a deliberate reconnect (the
     * user tapped Connect, or the service just started) and false when
     * this is an automatic retry — a manual attempt always starts the
     * backoff over, since it's a new reason to expect success.
     */
    fun connect(resetBackoff: Boolean = true) {
        reconnectHandler.removeCallbacks(reconnectRunnable)
        if (resetBackoff) reconnectDelayMs = RECONNECT_INITIAL_DELAY_MS

        socket.close()
        val host = Prefs.host(this)
        val port = Prefs.port(this)

        status = Status.CONNECTING
        socket.connect(
            host = host,
            port = port,
            onOpen = {
                reconnectDelayMs = RECONNECT_INITIAL_DELAY_MS
                status = Status.CONNECTED
            },
            onClip = { clip ->
                onClipReceived?.invoke()
                clipPlayer.enqueue(clip)
            },
            onError = {
                status = Status.DISCONNECTED
                scheduleReconnect()
            },
            onClosed = {
                status = Status.DISCONNECTED
                scheduleReconnect()
            },
        )
    }

    /**
     * Retries with exponential backoff (1s, 2s, 4s, … capped at 30s), as
     * long as the service is alive *and* [Prefs.autoReconnect] is on — the
     * only ways out are a successful connection (which resets the delay),
     * the user flipping the switch off, or [onDestroy]. With the switch
     * off this simply does nothing: the service stays DISCONNECTED until
     * [connect] is called again (the user reopening settings and saving,
     * or tapping the core).
     */
    private fun scheduleReconnect() {
        reconnectHandler.removeCallbacks(reconnectRunnable)
        if (!Prefs.autoReconnect(this)) return
        reconnectHandler.postDelayed(reconnectRunnable, reconnectDelayMs)
        reconnectDelayMs = (reconnectDelayMs * 2).coerceAtMost(RECONNECT_MAX_DELAY_MS)
    }

    private fun startForegroundWithNotification() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                NOTIFICATION_ID,
                buildNotification(),
                ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
            )
        } else {
            startForeground(NOTIFICATION_ID, buildNotification())
        }
    }

    private fun buildNotification(): Notification {
        val host = Prefs.host(this)
        val port = Prefs.port(this)
        val text = when (status) {
            Status.CONNECTING -> getString(R.string.connecting)
            Status.CONNECTED -> getString(R.string.connected, host, port)
            Status.DISCONNECTED ->
                if (Prefs.autoReconnect(this)) getString(R.string.disconnected)
                else getString(R.string.disconnected_manual)
        }
        return NotificationCompat.Builder(this, NOTIFICATION_CHANNEL_ID)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(text)
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    private fun updateNotification() {
        getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, buildNotification())
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                NOTIFICATION_CHANNEL_ID,
                getString(R.string.app_name),
                NotificationManager.IMPORTANCE_LOW,
            )
            getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
    }

    override fun onDestroy() {
        reconnectHandler.removeCallbacks(reconnectRunnable)
        socket.close()
        clipPlayer.release()
        releaseLocks()
        super.onDestroy()
    }
}
