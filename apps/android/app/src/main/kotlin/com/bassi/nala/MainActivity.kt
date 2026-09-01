package com.bassi.nala

import android.Manifest
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import com.bassi.nala.audio.Recorder
import com.bassi.nala.audio.WavWriter
import com.bassi.nala.settings.Prefs
import com.bassi.nala.ui.MicButton

/**
 * The only screen: connection status, a host:port field, and one circular
 * mic button that toggles recording. The connection itself and clip
 * playback live in [NalaService] — a foreground service this activity
 * binds to but doesn't own — so a reply that arrives, or is still playing,
 * survives this activity closing. Recording (the microphone) still only
 * happens while this activity is open.
 */
class MainActivity : AppCompatActivity() {

    private var nalaService: NalaService? = null
    private var bound = false

    private var recorder: Recorder? = null
    private var recording = false
    private val mainHandler = Handler(Looper.getMainLooper())

    private lateinit var textStatus: TextView
    private lateinit var textTurnStatus: TextView
    private lateinit var editHostPort: EditText
    private lateinit var micButton: MicButton

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            val service = (binder as NalaService.LocalBinder).service()
            nalaService = service
            bound = true
            service.onStatusChanged = { status -> runOnUi { updateStatusUi(status) } }
            service.onClipReceived = { runOnUi { setTurnStatus(getString(R.string.playing_reply)) } }
            updateStatusUi(service.status)
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            nalaService = null
            bound = false
        }
    }

    private val requestMicPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) {
                startRecording()
            } else {
                Toast.makeText(this, R.string.mic_permission_required, Toast.LENGTH_SHORT).show()
            }
        }

    // Without this, NalaService's foreground notification is silently
    // suppressed on Android 13+ — the service still runs, but the user
    // loses the only visible sign it's connected in the background.
    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        textStatus = findViewById(R.id.textStatus)
        textTurnStatus = findViewById(R.id.textTurnStatus)
        editHostPort = findViewById(R.id.editHostPort)
        micButton = findViewById(R.id.micButton)

        editHostPort.setText("${Prefs.host(this)}:${Prefs.port(this)}")

        findViewById<Button>(R.id.btnConnect).setOnClickListener { saveAndReconnect() }
        micButton.setOnClickListener { onRecordClicked() }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        val serviceIntent = Intent(this, NalaService::class.java)
        ContextCompat.startForegroundService(this, serviceIntent)
        bindService(serviceIntent, serviceConnection, Context.BIND_AUTO_CREATE)
    }

    private fun parseHostPort(): Pair<String, Int>? {
        val raw = editHostPort.text.toString().trim()
        val separatorIndex = raw.lastIndexOf(':')
        val port = raw.substring(separatorIndex + 1).toIntOrNull()
        if (separatorIndex <= 0 || port == null) return null
        return raw.substring(0, separatorIndex) to port
    }

    private fun saveAndReconnect() {
        val (host, port) = parseHostPort() ?: run {
            Toast.makeText(this, R.string.invalid_host_port, Toast.LENGTH_SHORT).show()
            return
        }
        Prefs.save(this, host, port)
        nalaService?.connect()
    }

    private fun onRecordClicked() {
        if (recording) {
            stopRecordingAndSend()
            return
        }

        if (ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            requestMicPermission.launch(Manifest.permission.RECORD_AUDIO)
            return
        }
        startRecording()
    }

    private fun startRecording() {
        recording = true
        micButton.recording = true
        micButton.amplitude = 0f
        setTurnStatus(getString(R.string.recording))

        val newRecorder = Recorder(onAmplitude = { amplitude -> runOnUi { micButton.amplitude = amplitude } })
        recorder = newRecorder
        newRecorder.start()
    }

    private fun stopRecordingAndSend() {
        recording = false
        micButton.recording = false
        micButton.amplitude = 0f

        val pcm = recorder?.stop() ?: ShortArray(0)
        recorder = null

        if (pcm.isEmpty()) {
            setTurnStatus(getString(R.string.nothing_recorded))
            return
        }

        setTurnStatus(getString(R.string.sending))
        val wav = WavWriter.wrap(pcm, Recorder.SAMPLE_RATE, channels = 1)
        val sent = nalaService?.socket?.sendUtterance(wav) ?: false
        if (!sent) {
            setTurnStatus(getString(R.string.not_connected))
        }
    }

    private fun updateStatusUi(status: NalaService.Status) {
        val host = Prefs.host(this)
        val port = Prefs.port(this)
        textStatus.text = when (status) {
            NalaService.Status.CONNECTING -> getString(R.string.connecting)
            NalaService.Status.CONNECTED -> getString(R.string.connected, host, port)
            NalaService.Status.DISCONNECTED -> getString(R.string.disconnected)
        }
    }

    private fun setTurnStatus(text: String) {
        textTurnStatus.text = text
    }

    private fun runOnUi(block: () -> Unit) {
        mainHandler.post(block)
    }

    override fun onDestroy() {
        recorder?.stop()
        if (bound) {
            nalaService?.onStatusChanged = null
            nalaService?.onClipReceived = null
            unbindService(serviceConnection)
            bound = false
        }
        // Deliberately not stopping NalaService here: the connection and
        // any in-flight clip playback must survive this activity closing.
        super.onDestroy()
    }
}
