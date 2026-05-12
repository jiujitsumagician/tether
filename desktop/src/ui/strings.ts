// The 14 strings that make up the entire pairing-flow vocabulary.
// Any user-visible text not in this file is a bug — see PAIRING.md.

export const STRINGS = {
  // PC home, before phone is seen
  "home.pc.idle": "Open Tether on your phone.",

  // Phone home, before PC is seen (lives here too so we can mirror
  // the table on the Android side without drift)
  "home.phone.idle": "Looking for your computer…",

  // Cascade status lines
  "cascade.mdns": "Looking on your Wi-Fi…",
  "cascade.fallback": "Trying another way…",
  "cascade.usb.prompt": "Connect your phone with any USB cable.",
  "cascade.usb.detected": "Got it. Finishing up…",
  "cascade.usb.debug": "Tap below to turn on USB debugging — it takes 10 seconds.",
  "cascade.hotspot": "Turn on your phone's hotspot. We'll connect to it automatically.",

  // Pairing card
  "pair.card.title": "Pair with {peer}?",
  "pair.card.subhead":
    "These three emojis should match on both screens. Tap Confirm on both.",
  "pair.card.confirm": "Confirm pairing",
  "pair.mismatch":
    "These emojis don't match. Don't confirm — start over from both apps.",
  "pair.success": "You're paired. You can close this.",
  "pair.manual": "Pair another way",
} as const;

export type StringKey = keyof typeof STRINGS;

export function t(key: StringKey, vars?: Record<string, string>): string {
  let s = STRINGS[key] as string;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      s = s.replace(`{${k}}`, v);
    }
  }
  return s;
}
