package io.tether.discovery

import android.content.Context
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withTimeoutOrNull

const val TETHER_UDP_PORT = 31413
const val TETHER_TLS_PORT = 31415
val TETHER_MAGIC = "TETHER1\n".toByteArray()

data class DiscoveredPeer(
    val deviceType: String,
    val name: String,
    val address: String,
    val port: Int,
    val via: Method,
) {
    enum class Method { MDNS, UDP_BROADCAST, SUBNET_PROBE, USB, MANUAL }
}

sealed interface CascadeEvent {
    data class Phase(val key: String) : CascadeEvent
    data object Exhausted : CascadeEvent
    data class Found(val peer: DiscoveredPeer) : CascadeEvent
}

data class CascadeOptions(
    val localDeviceName: String,
    val localDeviceType: String,
    val mdnsTimeoutMs: Long = 5_000,
    val udpTimeoutMs: Long = 3_000,
    val subnetTimeoutMs: Long = 2_000,
    val udpPort: Int = TETHER_UDP_PORT,
)

suspend fun runCascade(
    context: Context,
    options: CascadeOptions,
    events: Channel<CascadeEvent>,
): DiscoveredPeer? = coroutineScope {
    suspend fun emit(e: CascadeEvent) { events.send(e) }

    emit(CascadeEvent.Phase("cascade.mdns"))
    val mdns = withTimeoutOrNull(options.mdnsTimeoutMs) {
        MdnsClient.discover(context, options.localDeviceType)
    }
    if (mdns != null) {
        emit(CascadeEvent.Found(mdns))
        events.close()
        return@coroutineScope mdns
    }

    emit(CascadeEvent.Phase("cascade.fallback"))
    val udp = withTimeoutOrNull(options.udpTimeoutMs) {
        UdpListener.discover(options)
    }
    if (udp != null) {
        emit(CascadeEvent.Found(udp))
        events.close()
        return@coroutineScope udp
    }

    val subnet = withTimeoutOrNull(options.subnetTimeoutMs) {
        SubnetProbe.discover(context, options)
    }
    if (subnet != null) {
        emit(CascadeEvent.Found(subnet))
        events.close()
        return@coroutineScope subnet
    }

    // Android side has no native USB fallback — it's the desktop that
    // owns the cable. We instead show the "Connect your phone with any
    // USB cable" prompt and wait for the desktop's content provider
    // insertion (handled by PairingTokenProvider, which posts a
    // DiscoveredPeer via a static channel that this loop consumes).
    emit(CascadeEvent.Phase("cascade.usb.prompt"))
    val usb = PairingTokenProvider.awaitToken(60_000)
    if (usb != null) {
        emit(CascadeEvent.Phase("cascade.usb.detected"))
        events.close()
        return@coroutineScope usb
    }

    emit(CascadeEvent.Exhausted)
    events.close()
    null
}
