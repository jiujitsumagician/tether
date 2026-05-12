package io.tether.pairing

import android.app.Application
import android.content.Intent
import android.provider.Settings
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import io.tether.discovery.CascadeEvent
import io.tether.discovery.CascadeOptions
import io.tether.discovery.runCascade
import io.tether.store.PairedPcStore
import io.tether.transport.TlsClient
import io.tether.transport.WsChannel
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

sealed interface PairingUiState {
    data object Idle : PairingUiState
    data class Status(val statusKey: String, val allowManual: Boolean) : PairingUiState
    data class Card(val peerName: String, val emojis: List<String>) : PairingUiState
    data object ManualForm : PairingUiState
    data class Paired(val peerName: String) : PairingUiState
    data class Mismatch(val reason: String, val detail: String? = null) : PairingUiState
    data object Exhausted : PairingUiState
}

class PairingViewModel(
    private val app: Application,
    private val store: PairedPcStore,
) : AndroidViewModel(app) {

    private val _state = MutableStateFlow<PairingUiState>(PairingUiState.Idle)
    val state: StateFlow<PairingUiState> = _state.asStateFlow()

    private var runJob: Job? = null
    private val userConfirm = Channel<Unit>(Channel.CONFLATED)
    private val userMismatch = Channel<Unit>(Channel.CONFLATED)

    init {
        start()
    }

    fun start() {
        runJob?.cancel()
        runJob = viewModelScope.launch {
            // First: silent reconnect attempt if we already have a
            // paired PC.
            val known = store.list()
            if (known.isNotEmpty()) {
                if (tryReconnect(known)) return@launch
            }

            // Otherwise, run the full cascade.
            _state.value = PairingUiState.Status("cascade.mdns", allowManual = true)
            val options = CascadeOptions(
                localDeviceName = android.os.Build.MODEL ?: "Phone",
                localDeviceType = "phone",
            )
            val events = Channel<CascadeEvent>(Channel.UNLIMITED)
            val cascade = async {
                runCascade(app, options, events)
            }
            // Pump events into the UI state.
            for (evt in events) {
                when (evt) {
                    is CascadeEvent.Phase ->
                        _state.value = PairingUiState.Status(evt.key, allowManual = true)
                    is CascadeEvent.Exhausted ->
                        _state.value = PairingUiState.Exhausted
                    is CascadeEvent.Found -> Unit
                }
            }
            val peer = cascade.await() ?: run {
                _state.value = PairingUiState.Exhausted
                return@launch
            }
            handshakeWith(peer.address, peer.port, peer.name, pin = null)
        }
    }

    fun restart() {
        userConfirm.tryReceive(); userMismatch.tryReceive() // drain
        _state.value = PairingUiState.Idle
        start()
    }

    fun confirm() { userConfirm.trySend(Unit) }
    fun mismatch() { userMismatch.trySend(Unit) }

    fun openManualEntry() {
        runJob?.cancel()
        _state.value = PairingUiState.ManualForm
    }

    fun submitManual(address: String, pin: String) {
        runJob?.cancel()
        runJob = viewModelScope.launch {
            val (host, port) = parseHostPort(address) ?: run {
                _state.value = PairingUiState.Mismatch("protocol")
                return@launch
            }
            _state.value = PairingUiState.Status("cascade.fallback", allowManual = false)
            handshakeWith(host, port, "PC", pin)
        }
    }

    fun openDeveloperSettings() {
        val intent = Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        try {
            app.startActivity(intent)
        } catch (_: Exception) {
            val fallback = Intent(Settings.ACTION_DEVICE_INFO_SETTINGS).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            try { app.startActivity(fallback) } catch (_: Exception) {}
        }
    }

    private suspend fun tryReconnect(known: List<PairedPcStore.Entry>): Boolean =
        kotlinx.coroutines.coroutineScope {
            _state.value = PairingUiState.Status("cascade.mdns", allowManual = true)
            val options = CascadeOptions(
                localDeviceName = android.os.Build.MODEL ?: "Phone",
                localDeviceType = "phone",
                // Tighter timeouts for the silent path.
                mdnsTimeoutMs = 3_000,
                udpTimeoutMs = 1_500,
            )
            val events = Channel<CascadeEvent>(Channel.UNLIMITED)
            val peerJob = async {
                withTimeoutOrNull(6_000) { runCascade(app, options, events) }
            }
            for (evt in events) {
                // Don't surface status updates during reconnect — the UI
                // is allowed to look idle.
                if (evt is CascadeEvent.Phase) continue
            }
            val p = peerJob.await() ?: return@coroutineScope false
            // Bring up TLS and check fingerprint.
            try {
                val tls = TlsClient.dial(p.address, p.port)
                val fp = tls.peerCertSha256
                val match = known.firstOrNull { it.peerTlsCertSha256.contentEquals(fp) }
                if (match != null) {
                    _state.value = PairingUiState.Paired(match.peerDeviceName)
                    true
                } else {
                    tls.close()
                    false
                }
            } catch (_: Throwable) {
                false
            }
        }

    private suspend fun handshakeWith(
        host: String,
        port: Int,
        defaultPeerName: String,
        pin: String?,
    ) {
        try {
            val tls = TlsClient.dial(host, port)
            val peerCertFp = tls.peerCertSha256
            val ws = WsChannel.upgrade(tls, pin)

            val hs = Handshake()
            val ownCertFp = TlsClient.ownCertSha256(app)

            // hello
            val helloOut = Envelope(
                v = 1,
                type = "hello",
                id = 1,
                inReplyTo = null,
                body = Bodies.helloBody(
                    deviceType = "phone",
                    deviceName = android.os.Build.MODEL ?: "Phone",
                    protocolVersion = 1,
                    ecdhPubkey = hs.localPubBytes,
                    tlsCertSha256 = ownCertFp,
                ),
            )
            ws.sendEnvelope(helloOut)

            val peerHello = ws.recvEnvelope() ?: throw IllegalStateException("closed before hello")
            require(peerHello.type == "hello") { "expected hello, got ${peerHello.type}" }
            val peerPub = Bodies.readEcdhPubkey(peerHello.body)
            val peerReportedFp = Bodies.readTlsCertSha256(peerHello.body)
            if (!peerReportedFp.contentEquals(peerCertFp)) {
                _state.value = PairingUiState.Mismatch("protocol")
                ws.close(); return
            }

            val verifier = hs.derive(peerPub)
            val indices = indicesFromVerifier(verifier)
            val emojis = emojisFromVerifier(verifier)

            ws.sendEnvelope(Envelope(
                v = 1, type = "verify", id = 2, inReplyTo = peerHello.id,
                body = Bodies.verifyBody(verifier, indices, android.os.Build.MODEL ?: "Phone"),
            ))

            val peerVerify = ws.recvEnvelope() ?: throw IllegalStateException("closed before verify")
            require(peerVerify.type == "verify") { "expected verify, got ${peerVerify.type}" }
            val peerVerifier = Bodies.readFingerprint(peerVerify.body)
            if (!peerVerifier.contentEquals(verifier)) {
                _state.value = PairingUiState.Mismatch("protocol")
                ws.close(); return
            }

            val peerName = Bodies.readDeviceName(peerVerify.body)
            _state.value = PairingUiState.Card(peerName, emojis)

            // Wait for user confirm + peer confirm (or any mismatch).
            val result = withTimeoutOrNull(60_000) {
                var localSent = false
                var peerSeen = false
                while (true) {
                    val msg = race(
                        userConfirmAwait = !localSent,
                        peerRecv = { ws.recvEnvelope() },
                    )
                    when (msg) {
                        ConfirmEvent.UserConfirm -> {
                            ws.sendEnvelope(Envelope(
                                v = 1, type = "confirm", id = 3, inReplyTo = peerVerify.id,
                                body = Bodies.confirmBody(),
                            ))
                            localSent = true
                            if (peerSeen) break
                        }
                        ConfirmEvent.UserMismatch -> {
                            ws.sendEnvelope(Envelope(
                                v = 1, type = "mismatch", id = 4, inReplyTo = peerVerify.id,
                                body = Bodies.mismatchBody("user_mismatch"),
                            ))
                            throw IllegalStateException("user mismatch")
                        }
                        is ConfirmEvent.Peer -> {
                            when (msg.env.type) {
                                "confirm" -> { peerSeen = true; if (localSent) break }
                                "mismatch" -> {
                                    val reason = Bodies.readReason(msg.env.body)
                                    throw IllegalStateException("peer mismatch: $reason")
                                }
                                else -> { /* ignore */ }
                            }
                        }
                        null -> throw IllegalStateException("connection closed")
                    }
                }
                true
            }

            if (result == null) {
                ws.sendEnvelope(Envelope(
                    v = 1, type = "mismatch", id = 5, inReplyTo = null,
                    body = Bodies.mismatchBody("timeout"),
                ))
                _state.value = PairingUiState.Mismatch("timeout")
                ws.close(); return
            }

            store.add(PairedPcStore.Entry(
                peerDeviceType = Bodies.readDeviceType(peerHello.body),
                peerDeviceName = peerName,
                peerX25519Pubkey = peerPub,
                peerTlsCertSha256 = peerCertFp,
                pairedAt = System.currentTimeMillis() / 1000,
            ))
            _state.value = PairingUiState.Paired(peerName)
            // Give the user a beat to see the success, then close.
            delay(2_500)
            ws.close()
        } catch (e: Throwable) {
            // Surface the real exception under the category so we
            // can debug field failures from the screenshot alone,
            // not "Something didn't add up about the other device."
            android.util.Log.w("Tether", "handshake failed", e)
            val category = when {
                e.message?.contains("user_mismatch") == true -> "user_mismatch"
                e.message?.contains("timeout") == true -> "timeout"
                else -> "protocol"
            }
            _state.value = PairingUiState.Mismatch(category, e.toString())
        }
    }

    private sealed interface ConfirmEvent {
        data object UserConfirm : ConfirmEvent
        data object UserMismatch : ConfirmEvent
        data class Peer(val env: Envelope) : ConfirmEvent
    }

    private suspend fun race(
        userConfirmAwait: Boolean,
        peerRecv: suspend () -> Envelope?,
    ): ConfirmEvent? = kotlinx.coroutines.coroutineScope {
        // Re-entrant select. We want to listen on user channels AND
        // on the websocket simultaneously. The peer recv has to run
        // inside a child scope so its Deferred is awaitable from the
        // select block; coroutineScope here gives us that.
        val peerJob = async { peerRecv() }
        try {
            kotlinx.coroutines.selects.select {
                if (userConfirmAwait) {
                    userConfirm.onReceive { ConfirmEvent.UserConfirm as ConfirmEvent }
                }
                userMismatch.onReceive { ConfirmEvent.UserMismatch as ConfirmEvent }
                peerJob.onAwait { env -> env?.let { ConfirmEvent.Peer(it) } }
            }
        } finally {
            if (peerJob.isActive) peerJob.cancel()
        }
    }

    private fun parseHostPort(s: String): Pair<String, Int>? {
        val (h, p) = s.split(':', limit = 2).takeIf { it.size == 2 } ?: return null
        val port = p.toIntOrNull() ?: return null
        return h to port
    }

    class Factory(private val app: Application) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            val store = PairedPcStore(app)
            return PairingViewModel(app, store) as T
        }
    }
}
