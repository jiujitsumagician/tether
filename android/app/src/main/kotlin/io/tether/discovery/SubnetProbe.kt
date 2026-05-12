package io.tether.discovery

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.NetworkInterface

object SubnetProbe {
    suspend fun discover(
        context: Context,
        options: CascadeOptions,
    ): DiscoveredPeer? = withContext(Dispatchers.IO) {
        val targets = enumerate24HostsForLocalInterfaces()
        if (targets.isEmpty()) return@withContext null
        val socket = DatagramSocket(null).apply {
            reuseAddress = true
            bind(InetSocketAddress(0))
        }
        val payload = UdpListener.buildAnnounce(options)
        try {
            // Listener — break out the moment we get any non-self
            // response.
            val listener = async {
                val buf = ByteArray(1024)
                val inPacket = DatagramPacket(buf, buf.size)
                while (true) {
                    socket.soTimeout = 200
                    try {
                        socket.receive(inPacket)
                        val peer = UdpListener.parseAnnounce(
                            inPacket.data, inPacket.length,
                            inPacket.address.hostAddress ?: "",
                            options.localDeviceType,
                        )
                        if (peer != null) return@async peer
                    } catch (_: java.net.SocketTimeoutException) { continue }
                    catch (_: Throwable) { return@async null }
                }
                @Suppress("UNREACHABLE_CODE") null
            }
            coroutineScope {
                targets.chunked(32).forEach { chunk ->
                    chunk.map { ip ->
                        async {
                            try {
                                val p = DatagramPacket(payload, payload.size,
                                    InetAddress.getByName(ip), options.udpPort)
                                socket.send(p)
                            } catch (_: Throwable) { /* host unreachable */ }
                        }
                    }.awaitAll()
                    delay(20)
                }
            }
            listener.await()
        } finally {
            try { socket.close() } catch (_: Throwable) {}
        }
    }

    private fun enumerate24HostsForLocalInterfaces(): List<String> {
        val out = mutableListOf<String>()
        val ifaces = NetworkInterface.getNetworkInterfaces() ?: return out
        for (iface in ifaces) {
            if (!iface.isUp || iface.isLoopback) continue
            for (addr in iface.interfaceAddresses) {
                val ip = addr.address
                if (ip !is Inet4Address) continue
                val prefix = addr.networkPrefixLength.toInt()
                if (prefix < 24) continue
                val bytes = ip.address
                bytes[3] = 0
                val base = InetAddress.getByAddress(bytes)
                val baseOctets = base.address
                for (h in 1..254) {
                    baseOctets[3] = h.toByte()
                    out += InetAddress.getByAddress(baseOctets.copyOf()).hostAddress!!
                }
            }
        }
        return out
    }
}
