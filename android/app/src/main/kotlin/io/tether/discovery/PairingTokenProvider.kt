package io.tether.discovery

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.net.Uri
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull

/**
 * ContentProvider used by the desktop USB / ADB fallback. The desktop
 * pushes a one-time pairing token + the TLS port via:
 *
 *   adb shell content insert \
 *     --uri content://io.tether.pairing/token \
 *     --bind token:s:<base64> \
 *     --bind port:i:31415
 *
 * On insert we surface a [DiscoveredPeer] pointing at `localhost:<port>`
 * (the desktop will have set up an `adb reverse` tunnel) and the
 * cascade picks it up via [awaitToken].
 */
class PairingTokenProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun insert(uri: Uri, values: ContentValues?): Uri? {
        val token = values?.getAsString("token")
        val port = values?.getAsInteger("port") ?: TETHER_TLS_PORT
        if (token.isNullOrBlank()) return null
        val peer = DiscoveredPeer(
            deviceType = "pc",
            name = "PC (USB)",
            address = "127.0.0.1",
            port = port,
            via = DiscoveredPeer.Method.USB,
        )
        // Use trySend so we never block ADB's content-insert call.
        channel.trySend(peer)
        return uri.buildUpon().appendPath("ok").build()
    }

    // Stubbed CRUD — we only use `insert`.
    override fun query(
        uri: Uri,
        projection: Array<String>?,
        selection: String?,
        selectionArgs: Array<String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun getType(uri: Uri): String? = null

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<String>?): Int = 0

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<String>?,
    ): Int = 0

    companion object {
        // Conflated so a stale token gets overwritten by a fresh one.
        private val channel = Channel<DiscoveredPeer>(Channel.CONFLATED)

        suspend fun awaitToken(timeoutMs: Long): DiscoveredPeer? =
            withTimeoutOrNull(timeoutMs) { channel.receive() }

        @JvmStatic
        @Suppress("unused")
        fun drainBlocking(): DiscoveredPeer? = runBlocking {
            channel.tryReceive().getOrNull()
        }
    }
}
