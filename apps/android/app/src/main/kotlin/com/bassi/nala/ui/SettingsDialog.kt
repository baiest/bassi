package com.bassi.nala.ui

import android.content.Context
import android.widget.EditText
import android.widget.Toast
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.widget.SwitchCompat
import com.bassi.nala.R
import com.bassi.nala.settings.HostPort
import com.bassi.nala.settings.Prefs

/**
 * Host:port + auto-reconnect, behind one settings icon instead of always on
 * screen — this is touched once per network, not every time someone talks
 * to Nala. [onSave] receives the new host/port only when the user actually
 * saved (so the caller knows to reconnect); the auto-reconnect switch is
 * persisted directly since [com.bassi.nala.NalaService] reads it lazily.
 */
object SettingsDialog {

    fun show(context: Context, onSave: (host: String, port: Int) -> Unit) {
        val view = android.view.LayoutInflater.from(context).inflate(R.layout.dialog_settings, null)
        val editHostPort = view.findViewById<EditText>(R.id.editHostPort)
        val switchAutoReconnect = view.findViewById<SwitchCompat>(R.id.switchAutoReconnect)

        editHostPort.setText("${Prefs.host(context)}:${Prefs.port(context)}")
        switchAutoReconnect.isChecked = Prefs.autoReconnect(context)

        AlertDialog.Builder(context)
            .setTitle(R.string.settings)
            .setView(view)
            .setPositiveButton(R.string.save) { _, _ ->
                val parsed = HostPort.parse(editHostPort.text.toString())
                if (parsed == null) {
                    Toast.makeText(context, R.string.invalid_host_port, Toast.LENGTH_SHORT).show()
                    return@setPositiveButton
                }
                Prefs.setAutoReconnect(context, switchAutoReconnect.isChecked)
                onSave(parsed.first, parsed.second)
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }
}
