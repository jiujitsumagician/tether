package io.tether.discovery

import android.content.Context
import android.net.wifi.WifiManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import javax.jmdns.JmDNS
import javax.jmdns.ServiceEvent
import javax.jmdns.ServiceListener
import kotlin.coroutines.resume

private const val SERVICE_TYPE = "_tether._tcp.local."

object MdnsClient {
    suspend fun discover(context: Context, ourDeviceType: String): DiscoveredPeer? =
        withContext(Dispatchers.IO) {
            // Acquire a multicast lock — on most Android devices mDNS
            // packets are filtered out unless the WifiManager has a
            // held MulticastLock.
            val wifi = context.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            val lock = wifi?.createMulticastLock("tether.mdns")?.apply {
                setReferenceCounted(false)
                acquire()
            }
            val jmdns = JmDNS.create()
            try {
                suspendCancellableCoroutine<DiscoveredPeer?> { cont ->
                    val listener = object : ServiceListener {
                        override fun serviceAdded(event: ServiceEvent) {
                            // Resolve in case the underlying impl
                            // hasn't done it yet.
                            jmdns.requestServiceInfo(event.type, event.name, true)
                        }

                        override fun serviceRemoved(event: ServiceEvent) {}

                        override fun serviceResolved(event: ServiceEvent) {
                            val info = event.info ?: return
                            val t = info.getPropertyString("t") ?: return
                            if (t == ourDeviceType) return
                            val n = info.getPropertyString("n") ?: info.name
                            val p = info.getPropertyString("p")?.toIntOrNull() ?: info.port
                            val host = info.inet4Addresses.firstOrNull()?.hostAddress
                                ?: info.hostAddresses.firstOrNull()
                                ?: return
                            if (cont.isActive) {
                                cont.resume(
                                    DiscoveredPeer(
                                        deviceType = t,
                                        name = n,
                                        address = host,
                                        port = p,
                                        via = DiscoveredPeer.Method.MDNS,
                                    )
                                )
                            }
                        }
                    }
                    jmdns.addServiceListener(SERVICE_TYPE, listener)
                    cont.invokeOnCancellation {
                        jmdns.removeServiceListener(SERVICE_TYPE, listener)
                    }
                }
            } finally {
                try { jmdns.close() } catch (_: Throwable) {}
                try { lock?.release() } catch (_: Throwable) {}
            }
        }
}
