package io.tether.ui

// Mirror of desktop/src/ui/strings.ts. Any user-visible text not in
// this map is a bug — see PAIRING.md.

object Strings {
    const val HomePcIdle = "Open Tether on your phone."
    const val HomePhoneIdle = "Looking for your computer…"

    const val CascadeMdns = "Looking on your Wi-Fi…"
    const val CascadeFallback = "Trying another way…"
    const val CascadeUsbPrompt = "Connect your phone with any USB cable."
    const val CascadeUsbDetected = "Got it. Finishing up…"
    const val CascadeUsbDebug =
        "Tap below to turn on USB debugging — it takes 10 seconds."
    const val CascadeHotspot =
        "Turn on your phone's hotspot. We'll connect to it automatically."

    const val PairCardSubhead =
        "These three emojis should match on both screens. Tap Confirm on both."
    const val PairCardConfirm = "Confirm pairing"
    const val PairMismatch =
        "These emojis don't match. Don't confirm — start over from both apps."
    const val PairSuccess = "You're paired. You can close this."
    const val PairManual = "Pair another way"

    fun pairCardTitle(peer: String) = "Pair with $peer?"
}
