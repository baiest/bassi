package com.bassi.nala.settings

import android.content.Context

private const val PREFS_NAME = "nala_prefs"
private const val KEY_HOST = "host"
private const val KEY_PORT = "port"
private const val KEY_AUTO_RECONNECT = "auto_reconnect"

/** Placeholder — the user must point this at their own PC's LAN address. */
const val DEFAULT_HOST = "192.168.1.100"
const val DEFAULT_PORT = 4181
const val DEFAULT_AUTO_RECONNECT = true

/** Where `voice --serve`'s host/port (and the connection prefs) live between launches. */
object Prefs {
    fun host(context: Context): String =
        prefs(context).getString(KEY_HOST, DEFAULT_HOST) ?: DEFAULT_HOST

    fun port(context: Context): Int = prefs(context).getInt(KEY_PORT, DEFAULT_PORT)

    fun save(context: Context, host: String, port: Int) {
        prefs(context).edit()
            .putString(KEY_HOST, host)
            .putInt(KEY_PORT, port)
            .apply()
    }

    /** Whether `NalaService` should keep retrying indefinitely after a dropped connection. */
    fun autoReconnect(context: Context): Boolean =
        prefs(context).getBoolean(KEY_AUTO_RECONNECT, DEFAULT_AUTO_RECONNECT)

    fun setAutoReconnect(context: Context, enabled: Boolean) {
        prefs(context).edit().putBoolean(KEY_AUTO_RECONNECT, enabled).apply()
    }

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
}
