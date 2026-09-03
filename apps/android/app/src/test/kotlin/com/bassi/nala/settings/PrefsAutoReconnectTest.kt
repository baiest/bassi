package com.bassi.nala.settings

import android.content.Context
import android.content.SharedPreferences
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.mockito.Mockito.mock
import org.mockito.Mockito.`when`

/**
 * Exercises `Prefs.autoReconnect` against a fake in-memory `SharedPreferences`
 * (no Robolectric in this module) so the default and round-trip behavior are
 * covered without touching a real Android `Context`.
 */
class PrefsAutoReconnectTest {

    private class FakePrefs : SharedPreferences {
        val values = mutableMapOf<String, Any?>()

        override fun getBoolean(key: String?, defValue: Boolean): Boolean =
            values[key] as? Boolean ?: defValue

        override fun getString(key: String?, defValue: String?): String? =
            values[key] as? String ?: defValue

        override fun getInt(key: String?, defValue: Int): Int = values[key] as? Int ?: defValue

        override fun edit(): SharedPreferences.Editor = FakeEditor(this)

        override fun getAll(): MutableMap<String, *> = values
        override fun getStringSet(key: String?, defValues: MutableSet<String>?) = defValues
        override fun getLong(key: String?, defValue: Long) = defValue
        override fun getFloat(key: String?, defValue: Float) = defValue
        override fun contains(key: String?) = values.containsKey(key)
        override fun registerOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener?) {}
        override fun unregisterOnSharedPreferenceChangeListener(listener: SharedPreferences.OnSharedPreferenceChangeListener?) {}
    }

    private class FakeEditor(private val prefs: FakePrefs) : SharedPreferences.Editor {
        private val pending = mutableMapOf<String, Any?>()
        override fun putString(key: String?, value: String?) = apply { pending[key!!] = value }
        override fun putStringSet(key: String?, values: MutableSet<String>?) = apply { pending[key!!] = values }
        override fun putInt(key: String?, value: Int) = apply { pending[key!!] = value }
        override fun putLong(key: String?, value: Long) = apply { pending[key!!] = value }
        override fun putFloat(key: String?, value: Float) = apply { pending[key!!] = value }
        override fun putBoolean(key: String?, value: Boolean) = apply { pending[key!!] = value }
        override fun remove(key: String?) = apply { pending.remove(key) }
        override fun clear() = apply { prefs.values.clear() }
        override fun commit(): Boolean {
            prefs.values.putAll(pending)
            return true
        }
        override fun apply() {
            commit()
        }
    }

    private fun contextWithPrefs(prefs: SharedPreferences): Context {
        val context = mock(Context::class.java)
        `when`(context.getSharedPreferences("nala_prefs", Context.MODE_PRIVATE)).thenReturn(prefs)
        return context
    }

    @Test
    fun autoReconnectDefaultsToTrue() {
        val context = contextWithPrefs(FakePrefs())
        assertTrue(Prefs.autoReconnect(context))
    }

    @Test
    fun autoReconnectRoundTripsThroughSave() {
        val fake = FakePrefs()
        val context = contextWithPrefs(fake)

        Prefs.setAutoReconnect(context, false)

        assertFalse(Prefs.autoReconnect(context))
        assertEquals(false, fake.values["auto_reconnect"])
    }
}
