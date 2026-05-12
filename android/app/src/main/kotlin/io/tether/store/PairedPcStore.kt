package io.tether.store

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import org.json.JSONArray
import org.json.JSONObject
import android.util.Base64

/**
 * Persisted list of paired PCs.
 *
 * EncryptedSharedPreferences is used so the cert fingerprint + X25519
 * pubkey aren't readable from a backup or unprivileged ADB pull. The
 * JSON payload mirrors the desktop's `paired.json` layout.
 */
class PairedPcStore(context: Context) {

    data class Entry(
        val peerDeviceType: String,
        val peerDeviceName: String,
        val peerX25519Pubkey: ByteArray,
        val peerTlsCertSha256: ByteArray,
        val pairedAt: Long,
    )

    private val prefs: SharedPreferences = run {
        val key = MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build()
        EncryptedSharedPreferences.create(
            context,
            "tether-paired",
            key,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    @Synchronized
    fun list(): List<Entry> {
        val raw = prefs.getString(KEY, null) ?: return emptyList()
        val arr = JSONArray(raw)
        val out = mutableListOf<Entry>()
        for (i in 0 until arr.length()) {
            val o = arr.getJSONObject(i)
            out += Entry(
                peerDeviceType = o.getString("peer_device_type"),
                peerDeviceName = o.getString("peer_device_name"),
                peerX25519Pubkey = Base64.decode(o.getString("peer_x25519_pubkey"), Base64.NO_WRAP),
                peerTlsCertSha256 = Base64.decode(o.getString("peer_tls_cert_sha256"), Base64.NO_WRAP),
                pairedAt = o.getLong("paired_at"),
            )
        }
        return out
    }

    @Synchronized
    fun add(entry: Entry) {
        val current = list().filterNot { it.peerTlsCertSha256.contentEquals(entry.peerTlsCertSha256) }
        val all = current + entry
        val arr = JSONArray()
        for (e in all) {
            arr.put(JSONObject().apply {
                put("peer_device_type", e.peerDeviceType)
                put("peer_device_name", e.peerDeviceName)
                put("peer_x25519_pubkey", Base64.encodeToString(e.peerX25519Pubkey, Base64.NO_WRAP))
                put("peer_tls_cert_sha256", Base64.encodeToString(e.peerTlsCertSha256, Base64.NO_WRAP))
                put("paired_at", e.pairedAt)
            })
        }
        prefs.edit().putString(KEY, arr.toString()).apply()
    }

    @Synchronized
    fun forget(fp: ByteArray) {
        val remaining = list().filterNot { it.peerTlsCertSha256.contentEquals(fp) }
        val arr = JSONArray()
        for (e in remaining) {
            arr.put(JSONObject().apply {
                put("peer_device_type", e.peerDeviceType)
                put("peer_device_name", e.peerDeviceName)
                put("peer_x25519_pubkey", Base64.encodeToString(e.peerX25519Pubkey, Base64.NO_WRAP))
                put("peer_tls_cert_sha256", Base64.encodeToString(e.peerTlsCertSha256, Base64.NO_WRAP))
                put("paired_at", e.pairedAt)
            })
        }
        prefs.edit().putString(KEY, arr.toString()).apply()
    }

    private companion object {
        const val KEY = "paired_pcs"
    }
}
