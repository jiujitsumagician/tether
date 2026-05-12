package io.tether.discovery

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress

object UdpListener {
    /** Bind to the broadcast port + listen until a non-self peer is heard. */
    suspend fun discover(options: CascadeOptions): DiscoveredPeer? = withContext(Dispatchers.IO) {
        val socket = DatagramSocket(null).apply {
            reuseAddress = true
            broadcast = true
            bind(InetSocketAddress(options.udpPort))
        }
        try {
            // Send our own announce, broadcasted, every 800ms. The
            // first non-self packet we receive wins.
            val sendPayload = buildAnnounce(options)
            val broadcast = InetAddress.getByName("255.255.255.255")
            val outPacket = DatagramPacket(sendPayload, sendPayload.size, broadcast, options.udpPort)

            val buf = ByteArray(1024)
            val inPacket = DatagramPacket(buf, buf.size)

            var lastSend = 0L
            while (true) {
                val now = System.currentTimeMillis()
                if (now - lastSend > 800) {
                    try { socket.send(outPacket) } catch (_: Throwable) {}
                    lastSend = now
                }
                socket.soTimeout = 200
                try {
                    socket.receive(inPacket)
                    val peer = parseAnnounce(
                        inPacket.data,
                        inPacket.length,
                        inPacket.address.hostAddress ?: "",
                        options.localDeviceType,
                    )
                    if (peer != null) return@withContext peer
                } catch (_: java.net.SocketTimeoutException) {
                    continue
                }
            }
            @Suppress("UNREACHABLE_CODE")
            null
        } finally {
            try { socket.close() } catch (_: Throwable) {}
        }
    }

    fun buildAnnounce(options: CascadeOptions): ByteArray {
        val nonce = (Math.random() * 0xffffffffL).toLong().toString(16).padStart(8, '0')
        val text = buildString {
            append("TETHER1\n")
            append("type=announce\n")
            append("device_type=").append(options.localDeviceType).append('\n')
            append("device_name=").append(options.localDeviceName.replace('\n', ' ')).append('\n')
            append("port=").append(TETHER_TLS_PORT).append('\n')
            append("cert_fp_short=\n") // phone-side cert fp not yet known until first dial
            append("nonce=").append(nonce).append('\n')
        }
        return text.toByteArray()
    }

    fun parseAnnounce(
        bytes: ByteArray,
        len: Int,
        fromHost: String,
        ourDeviceType: String,
    ): DiscoveredPeer? {
        if (len < TETHER_MAGIC.size) return null
        for (i in TETHER_MAGIC.indices) if (bytes[i] != TETHER_MAGIC[i]) return null
        val text = String(bytes, 0, len)
        val map = mutableMapOf<String, String>()
        for (line in text.lines().drop(1)) {
            val eq = line.indexOf('=')
            if (eq <= 0) continue
            map[line.substring(0, eq)] = line.substring(eq + 1)
        }
        if (map["type"] != "announce") return null
        val theirType = map["device_type"] ?: return null
        if (theirType == ourDeviceType) return null
        val name = map["device_name"] ?: "Unknown"
        val port = map["port"]?.toIntOrNull() ?: return null
        return DiscoveredPeer(
            deviceType = theirType,
            name = name,
            address = fromHost,
            port = port,
            via = DiscoveredPeer.Method.UDP_BROADCAST,
        )
    }
}
